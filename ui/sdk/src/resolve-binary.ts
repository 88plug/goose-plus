import { accessSync, constants } from "node:fs";
import { delimiter, join } from "node:path";

const BINARY = process.platform === "win32" ? "goose-plus.exe" : "goose-plus";

function isExecutable(path: string): boolean {
  try {
    accessSync(path, constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

/**
 * Resolves the path to the goose-plus binary.
 *
 * Resolution order:
 *   1. `GOOSE_BINARY` environment variable (explicit override)
 *   2. `goose-plus` on PATH, where download_cli.sh installs it
 *
 * @throws if no binary can be found
 */
export function resolveGooseBinary(): string {
  const envBinary = process.env.GOOSE_BINARY;
  if (envBinary) return envBinary;

  const searchPath = process.env.PATH ?? "";
  for (const dir of searchPath.split(delimiter)) {
    if (!dir) continue;
    const candidate = join(dir, BINARY);
    if (isExecutable(candidate)) return candidate;
  }

  throw new Error(
    `${BINARY} was not found on PATH. Install it, or set GOOSE_BINARY to the path of a ${BINARY} binary.`,
  );
}
