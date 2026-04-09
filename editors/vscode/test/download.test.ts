import * as path from "node:path";
import { describe, expect, it, vi, beforeEach } from "vitest";

let mockFiles: Record<string, string> = {};
let mockExecOutput = "";
let mockExecError = false;

vi.mock("node:fs", () => ({
  existsSync: (p: string) => p in mockFiles,
  readFileSync: (p: string) => {
    if (p in mockFiles) return mockFiles[p];
    throw new Error("ENOENT");
  },
  writeFileSync: (p: string, content: string) => {
    mockFiles[p] = content;
  },
  unlinkSync: (p: string) => {
    delete mockFiles[p];
  },
  mkdirSync: () => {},
}));

vi.mock("node:child_process", () => ({
  execFileSync: () => {
    if (mockExecError) throw new Error("exec failed");
    return mockExecOutput;
  },
}));

vi.mock("vscode", () => ({
  extensions: {
    getExtension: () => ({
      packageJSON: { version: "2.26.0" },
    }),
  },
}));

import { getInstalledBinaryPath, getBinaryVersion } from "../src/download.js";

const fakeContext = {
  globalStorageUri: { fsPath: "/storage" },
} as any;

const binDir = path.join("/storage", "bin");
const lspPath = path.join(binDir, "fallow-lsp");
const cliPath = path.join(binDir, "fallow");
const versionPath = path.join(binDir, ".fallow-version");

describe("getBinaryVersion", () => {
  beforeEach(() => {
    mockExecOutput = "";
    mockExecError = false;
  });

  it("parses version from fallow-lsp output", () => {
    mockExecOutput = "fallow-lsp 2.25.0\n";
    expect(getBinaryVersion("/bin/fallow-lsp")).toBe("2.25.0");
  });

  it("returns null on exec failure", () => {
    mockExecError = true;
    expect(getBinaryVersion("/bin/fallow-lsp")).toBeNull();
  });

  it("returns null on unparseable output", () => {
    mockExecOutput = "unknown";
    expect(getBinaryVersion("/bin/fallow-lsp")).toBeNull();
  });
});

describe("getInstalledBinaryPath", () => {
  beforeEach(() => {
    mockFiles = {};
    mockExecOutput = "";
    mockExecError = false;
  });

  it("returns null when no binary exists", () => {
    expect(getInstalledBinaryPath(fakeContext)).toBeNull();
  });

  it("returns path when version marker matches", () => {
    mockFiles[lspPath] = "";
    mockFiles[versionPath] = "2.26.0";

    expect(getInstalledBinaryPath(fakeContext)).toBe(lspPath);
  });

  it("returns null and deletes stale binary when marker version differs", () => {
    mockFiles[lspPath] = "";
    mockFiles[cliPath] = "";
    mockFiles[versionPath] = "2.25.0";

    expect(getInstalledBinaryPath(fakeContext)).toBeNull();
    expect(mockFiles[lspPath]).toBeUndefined();
    expect(mockFiles[cliPath]).toBeUndefined();
    expect(mockFiles[versionPath]).toBeUndefined();
  });

  it("falls back to --version when no marker exists", () => {
    mockFiles[lspPath] = "";
    mockExecOutput = "fallow-lsp 2.26.0\n";

    expect(getInstalledBinaryPath(fakeContext)).toBe(lspPath);
  });

  it("treats unknown version as stale (null --version, no marker)", () => {
    mockFiles[lspPath] = "";
    mockExecError = true;

    expect(getInstalledBinaryPath(fakeContext)).toBeNull();
    expect(mockFiles[lspPath]).toBeUndefined();
  });

  it("treats mismatched --version as stale when no marker", () => {
    mockFiles[lspPath] = "";
    mockExecOutput = "fallow-lsp 2.24.0\n";

    expect(getInstalledBinaryPath(fakeContext)).toBeNull();
    expect(mockFiles[lspPath]).toBeUndefined();
  });
});
