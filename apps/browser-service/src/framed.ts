/**
 * Bound NDJSON frame readers so peers cannot force unbounded buffering.
 */

export const MAX_FRAME_BYTES = 1_048_576;

/**
 * Yield newline-delimited UTF-8 frames, rejecting frames larger than `maxBytes`
 * before they are fully materialized as strings.
 */
export async function* cappedLines(
  stream: AsyncIterable<Uint8Array | string | Buffer>,
  maxBytes = MAX_FRAME_BYTES,
): AsyncGenerator<string> {
  let buffer = Buffer.alloc(0);
  for await (const chunk of stream) {
    const bytes = Buffer.isBuffer(chunk)
      ? chunk
      : typeof chunk === "string"
        ? Buffer.from(chunk)
        : Buffer.from(chunk);
    if (buffer.length + bytes.length > maxBytes && !buffer.includes(0x0a) && !bytes.includes(0x0a)) {
      throw new Error(`frame exceeds ${maxBytes} bytes`);
    }
    buffer = buffer.length === 0 ? bytes : Buffer.concat([buffer, bytes]);
    while (true) {
      const newline = buffer.indexOf(0x0a);
      if (newline < 0) {
        if (buffer.length > maxBytes) {
          throw new Error(`frame exceeds ${maxBytes} bytes`);
        }
        break;
      }
      let line = buffer.subarray(0, newline);
      buffer = buffer.subarray(newline + 1);
      if (line.length > 0 && line[line.length - 1] === 0x0d) {
        line = line.subarray(0, line.length - 1);
      }
      if (line.length > maxBytes) {
        throw new Error(`frame exceeds ${maxBytes} bytes`);
      }
      yield line.toString("utf8");
    }
  }
  if (buffer.length > 0) {
    if (buffer.length > maxBytes) {
      throw new Error(`frame exceeds ${maxBytes} bytes`);
    }
    let line = buffer;
    if (line.length > 0 && line[line.length - 1] === 0x0d) {
      line = line.subarray(0, line.length - 1);
    }
    yield line.toString("utf8");
  }
}
