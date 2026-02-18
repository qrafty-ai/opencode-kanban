#!/usr/bin/env node

import { spawn } from "node:child_process";
import { accessSync, chmodSync, constants, existsSync } from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const require = createRequire(import.meta.url);

const PLATFORM_PACKAGE_BY_TARGET = {
  "x86_64-unknown-linux-gnu": "@qrafty-ai/opencode-kanban-linux-x64",
  "aarch64-apple-darwin": "@qrafty-ai/opencode-kanban-darwin-arm64",
};

function detectTargetTriple() {
  const { platform, arch } = process;

  if (platform === "linux" && arch === "x64") {
    return "x86_64-unknown-linux-gnu";
  }

  if (platform === "darwin" && arch === "x64") {
    throw new Error("darwin-x64 is not supported. Please use macOS with Apple Silicon (arm64).");
  }

  if (platform === "darwin" && arch === "arm64") {
    return "aarch64-apple-darwin";
  }

  throw new Error(`Unsupported platform: ${platform} (${arch})`);
}

function detectPackageManager() {
  const userAgent = process.env.npm_config_user_agent || "";
  if (/\bbun\//.test(userAgent)) {
    return "bun";
  }
  return "npm";
}

const targetTriple = detectTargetTriple();
const platformPackage = PLATFORM_PACKAGE_BY_TARGET[targetTriple];
const binaryName = process.platform === "win32" ? "opencode-kanban.exe" : "opencode-kanban";
const localVendorRoot = path.join(__dirname, "..", "vendor");
const localBinaryPath = path.join(
  localVendorRoot,
  targetTriple,
  "opencode-kanban",
  binaryName,
);

let vendorRoot;
try {
  const packageJsonPath = require.resolve(`${platformPackage}/package.json`);
  vendorRoot = path.join(path.dirname(packageJsonPath), "vendor");
} catch {
  if (existsSync(localBinaryPath)) {
    vendorRoot = localVendorRoot;
  }
}

if (!vendorRoot) {
  const packageManager = detectPackageManager();
  const updateCommand =
    packageManager === "bun"
      ? "bun install -g @qrafty-ai/opencode-kanban@latest"
      : "npm install -g @qrafty-ai/opencode-kanban@latest";
  throw new Error(
    `Missing optional dependency ${platformPackage}. Reinstall opencode-kanban: ${updateCommand}`,
  );
}

const binaryPath = path.join(vendorRoot, targetTriple, "opencode-kanban", binaryName);

function ensureExecutable(pathToBinary) {
  if (process.platform === "win32") {
    return;
  }

  try {
    accessSync(pathToBinary, constants.X_OK);
    return;
  } catch {
  }

  try {
    chmodSync(pathToBinary, 0o755);
    accessSync(pathToBinary, constants.X_OK);
  } catch (error) {
    throw new Error(`Binary is not executable: ${pathToBinary}`, { cause: error });
  }
}

ensureExecutable(binaryPath);

const child = spawn(binaryPath, process.argv.slice(2), {
  stdio: "inherit",
  env: process.env,
});

child.on("error", (error) => {
  console.error(error);
  process.exit(1);
});

const forwardSignal = (signal) => {
  if (child.killed) {
    return;
  }
  try {
    child.kill(signal);
  } catch {
  }
};

["SIGINT", "SIGTERM", "SIGHUP"].forEach((signal) => {
  process.on(signal, () => forwardSignal(signal));
});

const childResult = await new Promise((resolve) => {
  child.on("exit", (code, signal) => {
    if (signal) {
      resolve({ type: "signal", signal });
      return;
    }
    resolve({ type: "code", exitCode: code ?? 1 });
  });
});

if (childResult.type === "signal") {
  process.kill(process.pid, childResult.signal);
} else {
  process.exit(childResult.exitCode);
}
