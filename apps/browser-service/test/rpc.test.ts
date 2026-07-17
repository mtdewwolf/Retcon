import { describe, expect, test } from "bun:test";
import { PROTOCOL_VERSION } from "../src/generated/protocol-v1";

describe("generated protocol v1", () => {
  test("exports protocol version 1", () => {
    expect(PROTOCOL_VERSION).toBe(1);
  });

  test("method catalog includes core.health", async () => {
    const schema = await Bun.file(
      new URL("../../../schemas/protocol/v1.json", import.meta.url),
    ).json();
    expect(schema.methodCatalog.core).toContain("core.health");
    expect(schema.methodCatalog.browser).toContain("browser.call");
  });
});
