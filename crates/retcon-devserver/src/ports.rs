//! Deterministic localhost port reservation.

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use crate::DevServerError;

const DEFAULT_FIRST_PORT: u16 = 3_000;
const DEFAULT_LAST_PORT: u16 = 3_999;
static DEFAULT_STATE: OnceLock<Arc<PortState>> = OnceLock::new();

#[derive(Debug)]
struct PortState {
    claimed: Mutex<HashMap<u16, u64>>,
    next_token: AtomicU64,
}

/// Shared collision-preventing deterministic port allocator.
#[derive(Debug, Clone)]
pub struct PortAllocator {
    first: u16,
    last: u16,
    state: Arc<PortState>,
}

impl Default for PortAllocator {
    fn default() -> Self {
        Self {
            first: DEFAULT_FIRST_PORT,
            last: DEFAULT_LAST_PORT,
            state: DEFAULT_STATE
                .get_or_init(|| {
                    Arc::new(PortState {
                        claimed: Mutex::new(HashMap::new()),
                        next_token: AtomicU64::new(1),
                    })
                })
                .clone(),
        }
    }
}

impl PortAllocator {
    /// Create an allocator for an inclusive non-zero port range.
    pub fn new(first: u16, last: u16) -> Result<Self, DevServerError> {
        if first == 0 || first > last {
            return Err(DevServerError::InvalidPortRange { first, last });
        }
        Ok(Self {
            first,
            last,
            state: Arc::new(PortState {
                claimed: Mutex::new(HashMap::new()),
                next_token: AtomicU64::new(1),
            }),
        })
    }

    /// Reserve an available port, using stable project/worktree affinity.
    pub fn reserve(
        &self,
        project_key: &str,
        worktree_key: &str,
        requested: Option<u16>,
        allow_alternate: bool,
    ) -> Result<PortReservation, DevServerError> {
        if let Some(port) = requested {
            match self.try_reserve(port) {
                Ok(reservation) => return Ok(reservation),
                Err(_) if allow_alternate => {}
                Err(_) => return Err(DevServerError::PortUnavailable(port)),
            }
        }

        let count = u32::from(self.last) - u32::from(self.first) + 1;
        let offset = stable_hash(project_key, worktree_key) % u64::from(count);
        for step in 0..count {
            let offset = (offset + u64::from(step)) % u64::from(count);
            let offset = u32::try_from(offset).map_err(|_| DevServerError::InvalidPortRange {
                first: self.first,
                last: self.last,
            })?;
            let port = u32::from(self.first) + offset;
            let port = u16::try_from(port).map_err(|_| DevServerError::InvalidPortRange {
                first: self.first,
                last: self.last,
            })?;
            if Some(port) == requested {
                continue;
            }
            if let Ok(reservation) = self.try_reserve(port) {
                return Ok(reservation);
            }
        }
        Err(DevServerError::NoAvailablePort {
            first: self.first,
            last: self.last,
        })
    }

    fn try_reserve(&self, port: u16) -> Result<PortReservation, DevServerError> {
        if port == 0 {
            return Err(DevServerError::PortUnavailable(port));
        }
        let mut claimed = self
            .state
            .claimed
            .lock()
            .map_err(|_| DevServerError::RuntimeState)?;
        if claimed.contains_key(&port) {
            return Err(DevServerError::PortUnavailable(port));
        }
        let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port))
            .map_err(|_| DevServerError::PortUnavailable(port))?;
        listener
            .set_nonblocking(true)
            .map_err(|error| DevServerError::Io("configure port reservation", error))?;
        let token = self.state.next_token.fetch_add(1, Ordering::Relaxed);
        claimed.insert(port, token);
        drop(claimed);
        Ok(PortReservation {
            allocator: self.clone(),
            port,
            token,
            listener: Some(listener),
        })
    }
}

/// Exclusive logical port claim with an initial OS socket reservation.
#[derive(Debug)]
pub struct PortReservation {
    allocator: PortAllocator,
    port: u16,
    token: u64,
    listener: Option<TcpListener>,
}

impl PortReservation {
    /// Reserved localhost port.
    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }

    pub(crate) fn release_socket_for_child(&mut self) {
        self.listener.take();
    }
}

impl Drop for PortReservation {
    fn drop(&mut self) {
        if let Ok(mut claimed) = self.allocator.state.claimed.lock()
            && claimed.get(&self.port) == Some(&self.token)
        {
            claimed.remove(&self.port);
        }
    }
}

fn stable_hash(project_key: &str, worktree_key: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in project_key
        .bytes()
        .chain([0xff])
        .chain(worktree_key.bytes())
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn allocation_is_deterministic_and_avoids_live_collisions() {
        let allocator = PortAllocator::new(41_000, 41_010).unwrap();
        let first = allocator
            .reserve("project", "worktree", None, true)
            .unwrap();
        let preferred = first.port();
        let alternate = allocator
            .reserve("project", "worktree", None, true)
            .unwrap();
        assert_ne!(alternate.port(), preferred);
        drop(alternate);
        drop(first);
        let repeated = allocator
            .reserve("project", "worktree", None, true)
            .unwrap();
        assert_eq!(repeated.port(), preferred);
    }

    #[test]
    fn requested_collision_can_fail_or_select_an_alternate() {
        let allocator = PortAllocator::new(41_020, 41_025).unwrap();
        let held = allocator
            .reserve("one", "one", Some(41_022), false)
            .unwrap();
        assert!(matches!(
            allocator.reserve("two", "two", Some(41_022), false),
            Err(DevServerError::PortUnavailable(41_022))
        ));
        let alternate = allocator.reserve("two", "two", Some(41_022), true).unwrap();
        assert_ne!(alternate.port(), held.port());
    }
}
