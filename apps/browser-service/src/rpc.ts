/**
 * RPC client skeleton.
 *
 * In Phase 4 the browser service connects to retcon-core over the local
 * transport (a named pipe on Windows) using types generated from
 * `schemas/protocol/`. Until then this module only defines the connection
 * surface so `main.ts` has a stable seam to grow against.
 */

import { logger } from "./logging";

export interface RpcClient {
  /** Whether a transport connection to retcon-core is established. */
  readonly connected: boolean;
  /** Disconnect and release the transport. */
  close(): Promise<void>;
}

/**
 * Connect to the retcon-core service.
 *
 * Phase 1 stub: records the intent and returns a disconnected client. The
 * real named-pipe transport, authentication handshake, and heartbeat land
 * with the protocol work in Phase 4.
 */
export function connect(pipeName: string | undefined): RpcClient {
  logger.info("rpc transport not yet implemented; running standalone", {
    pipe: pipeName ?? null,
  });
  return {
    connected: false,
    close: async () => {
      /* nothing to release yet */
    },
  };
}
