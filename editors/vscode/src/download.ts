import { execFileSync } from "node:child_process";
import * as fs from "node:fs";
import type { IncomingMessage } from "node:http";
import * as https from "node:https";
import * as os from "node:os";
import * as path from "node:path";
// VS Code injects this module into the extension host at runtime.
// fallow-ignore-next-line unlisted-dependency
import * as vscode from "vscode";
import { getExecutableExtension } from "./binary-utils.js";

const GITHUB_REPO = "fallow-rs/fallow";
const LSP_BINARY_NAME = "fallow-lsp";
const CLI_BINARY_NAME = "fallow";
const VERSION_FILE = ".fallow-version";

interface GithubRelease {
  readonly tag_name: string;
  readonly assets: ReadonlyArray<{
    readonly name: string;
    readonly browser_download_url: string;
  }>;
}

const REQUEST_HEADERS = { "User-Agent": "fallow-vscode" };

const getPlatformTarget = (): string | null => {
  const arch = os.arch();
  const platform = os.platform();

  if (platform === "darwin" && arch === "arm64") return "darwin-arm64";
  if (platform === "darwin" && arch === "x64") return "darwin-x64";
  if (platform === "linux" && arch === "x64") return "linux-x64-gnu";
  if (platform === "linux" && arch === "arm64") return "linux-arm64-gnu";
  if (platform === "win32" && arch === "x64") return "win32-x64-msvc";

  return null;
};

const withRedirects = <T>(
  url: string,
  handleResponse: (response: IncomingMessage) => Promise<T>
): Promise<T> =>
  new Promise((resolve, reject) => {
    const request = https.get(
      url,
      { headers: REQUEST_HEADERS },
      (response) => {
        if (
          response.statusCode &&
          response.statusCode >= 300 &&
          response.statusCode < 400 &&
          response.headers.location
        ) {
          response.resume();
          withRedirects(response.headers.location, handleResponse).then(
            resolve,
            reject
          );
          return;
        }

        if (response.statusCode && response.statusCode >= 400) {
          response.resume();
          reject(new Error(`HTTP ${response.statusCode}`));
          return;
        }

        void handleResponse(response).then(resolve, reject);
      }
    );

    request.on("error", reject);
  });

const httpsGet = (url: string): Promise<string> =>
  withRedirects(url, async (response) => {
    const chunks: Buffer[] = [];

    return await new Promise<string>((resolve, reject) => {
      response.on("data", (chunk: Buffer) => chunks.push(chunk));
      response.on("end", () => resolve(Buffer.concat(chunks).toString()));
      response.on("error", reject);
    });
  });

const httpsDownload = (url: string, dest: string): Promise<void> =>
  withRedirects(
    url,
    async (response) =>
      await new Promise<void>((resolve, reject) => {
        const file = fs.createWriteStream(dest);
        response.pipe(file);
        file.on("finish", () => {
          file.close();
          resolve();
        });
        file.on("error", (err) => {
          fs.unlink(dest, () => {});
          reject(err);
        });
      })
  );

const getInstallDir = (context: vscode.ExtensionContext): string => {
  const dir = path.join(context.globalStorageUri.fsPath, "bin");
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true });
  }
  return dir;
};

const writeVersionMarker = (dir: string, version: string): void => {
  try {
    fs.writeFileSync(path.join(dir, VERSION_FILE), version, "utf-8");
  } catch {
    // Best-effort — next activation falls back to --version
  }
};

const readVersionMarker = (dir: string): string | null => {
  try {
    return fs.readFileSync(path.join(dir, VERSION_FILE), "utf-8").trim() || null;
  } catch {
    return null;
  }
};

/** Query the version of a fallow binary. Returns the version string or null. */
export const getBinaryVersion = (binaryPath: string): string | null => {
  try {
    // execFileSync is safe (no shell injection) — binary path is from our own storage dir.
    const output = execFileSync(binaryPath, ["--version"], {
      timeout: 5000,
      encoding: "utf-8",
    });
    // Output format: "fallow-lsp 2.18.3" or "fallow 2.18.3"
    const match = output.trim().match(/(\d+\.\d+\.\d+)/);
    return match?.[1] ?? null;
  } catch {
    return null;
  }
};

export const getInstalledBinaryPath = (
  context: vscode.ExtensionContext,
  outputChannel?: vscode.OutputChannel
): string | null => {
  const dir = getInstallDir(context);
  const lspPath = path.join(
    dir,
    `${LSP_BINARY_NAME}${getExecutableExtension()}`
  );
  if (!fs.existsSync(lspPath)) {
    return null;
  }

  // Check if the installed binary matches the extension version.
  // Without this, a binary downloaded by a previous extension version
  // persists forever, causing version skew between LSP diagnostics and CLI.
  const extensionVersion =
    vscode.extensions.getExtension("fallow-rs.fallow-vscode")?.packageJSON
      ?.version as string | undefined;
  if (extensionVersion) {
    // Check version marker file first (written by downloadBinary), then fall
    // back to running --version for binaries downloaded before the marker existed.
    // If neither resolves, treat the binary as stale to avoid silent version skew.
    const binaryVersion = readVersionMarker(dir) ?? getBinaryVersion(lspPath);
    if (binaryVersion !== extensionVersion) {
      outputChannel?.appendLine(
        `Fallow: installed binary is v${binaryVersion ?? "unknown"}, extension is v${extensionVersion}. Re-downloading.`
      );
      // Remove stale binaries and version marker to force a fresh download
      try {
        fs.unlinkSync(lspPath);
        const cliPath = path.join(
          dir,
          `${CLI_BINARY_NAME}${getExecutableExtension()}`
        );
        if (fs.existsSync(cliPath)) {
          fs.unlinkSync(cliPath);
        }
        const versionPath = path.join(dir, VERSION_FILE);
        if (fs.existsSync(versionPath)) {
          fs.unlinkSync(versionPath);
        }
      } catch {
        // If deletion fails, still return null to trigger download
      }
      return null;
    }
  }

  return lspPath;
};

/** Download a single binary asset from a GitHub release. Returns the dest path or null. */
const downloadAsset = async (
  release: GithubRelease,
  binaryName: string,
  target: string,
  dir: string
): Promise<string | null> => {
  const extension = getExecutableExtension();
  const assetName = `${binaryName}-${target}${extension}`;
  const asset = release.assets.find((a) => a.name === assetName);

  if (!asset) {
    return null;
  }

  const destPath = path.join(dir, `${binaryName}${extension}`);
  await httpsDownload(asset.browser_download_url, destPath);

  if (os.platform() !== "win32") {
    fs.chmodSync(destPath, 0o755);
  }

  return destPath;
};

export const downloadBinary = async (
  context: vscode.ExtensionContext
): Promise<string | null> => {
  const target = getPlatformTarget();
  if (!target) {
    void vscode.window.showErrorMessage(
      `Fallow: unsupported platform ${os.platform()}-${os.arch()}`
    );
    return null;
  }

  return vscode.window.withProgress(
    {
      location: vscode.ProgressLocation.Notification,
      title: "Fallow: Downloading binaries...",
      cancellable: false,
    },
    async () => {
      try {
        const releaseJson = await httpsGet(
          `https://api.github.com/repos/${GITHUB_REPO}/releases/latest`
        );
        const release: GithubRelease = JSON.parse(releaseJson);
        const dir = getInstallDir(context);

        // Download LSP binary (required)
        const lspPath = await downloadAsset(release, LSP_BINARY_NAME, target, dir);
        if (!lspPath) {
          void vscode.window.showErrorMessage(
            `Fallow: no LSP binary found for ${target} in release ${release.tag_name}`
          );
          return null;
        }

        // Write version marker so future activations can detect stale binaries
        // without needing to execute them.
        const extensionVersion =
          vscode.extensions.getExtension("fallow-rs.fallow-vscode")?.packageJSON
            ?.version as string | undefined;
        if (extensionVersion) {
          writeVersionMarker(dir, extensionVersion);
        }

        // Download CLI binary (best-effort — tree views and commands need it)
        const cliPath = await downloadAsset(release, CLI_BINARY_NAME, target, dir);
        if (cliPath) {
          void vscode.window.showInformationMessage(
            `Fallow: ${release.tag_name} installed (LSP + CLI).`
          );
        } else {
          void vscode.window.showInformationMessage(
            `Fallow: LSP ${release.tag_name} installed. CLI binary not found in release — tree views require the fallow CLI in PATH.`
          );
        }

        return lspPath;
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        void vscode.window.showErrorMessage(
          `Fallow: failed to download binaries: ${message}`
        );
        return null;
      }
    }
  );
};
