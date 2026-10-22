import { describe, expect, it } from "vitest";
import {
  blankEncryptedPrefixEnvValues,
  encryptedPrefixMessage,
  envKeysWithEncryptedPrefix,
} from "./encryptedEnvValues";

describe("envKeysWithEncryptedPrefix", () => {
  it("lists only string values that start with the prefix", () => {
    expect(
      envKeysWithEncryptedPrefix({
        TOKEN: "enc:v1:AAAA",
        BARE: "enc:v1:",
        PLAIN: "value",
        MENTION: "see enc:v1: later",
        NUMBER: 7,
      }),
    ).toEqual(["TOKEN", "BARE"]);
  });

  it("returns nothing for a missing or malformed env", () => {
    expect(envKeysWithEncryptedPrefix(undefined)).toEqual([]);
    expect(envKeysWithEncryptedPrefix("enc:v1:AAAA")).toEqual([]);
    expect(envKeysWithEncryptedPrefix(["enc:v1:AAAA"])).toEqual([]);
  });
});

describe("encryptedPrefixMessage", () => {
  it("matches the backend refusal", () => {
    expect(encryptedPrefixMessage("TOKEN")).toBe(
      "Env value for 'TOKEN' cannot start with 'enc:v1:'. That prefix is reserved for encrypted values.",
    );
  });
});

describe("blankEncryptedPrefixEnvValues", () => {
  it("clears prefixed values, keeps their keys, and leaves the input alone", () => {
    const input = { command: "npx", env: { TOKEN: "enc:v1:AAAA", REGION: "eu" } };
    const { config, clearedKeys } = blankEncryptedPrefixEnvValues(input);
    expect(clearedKeys).toEqual(["TOKEN"]);
    expect(config).toEqual({ command: "npx", env: { TOKEN: "", REGION: "eu" } });
    expect(input.env.TOKEN).toBe("enc:v1:AAAA");
  });

  it("returns the same config when nothing is prefixed", () => {
    const input = { command: "npx", env: { REGION: "eu" } };
    const result = blankEncryptedPrefixEnvValues(input);
    expect(result.clearedKeys).toEqual([]);
    expect(result.config).toBe(input);
  });
});
