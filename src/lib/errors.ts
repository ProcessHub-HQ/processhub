const RAW_PATTERNS: Array<{ needles: string[]; message: string }> = [
  {
    needles: [
      "no such file or directory",
      "cannot find the file",
      "the system cannot find the file",
      "os error 2",
      "enoent",
    ],
    message:
      "A file or folder couldn't be found. Check the executable and working directory.",
  },
  {
    needles: [
      "permission denied",
      "access is denied",
      "operation not permitted",
      "os error 13",
      "os error 1",
      "os error 5",
      "eacces",
      "eperm",
    ],
    message:
      "ProcessHub doesn't have permission to do that. Check the file's permissions and try again.",
  },
  {
    needles: ["text file busy", "os error 26", "etxtbsy"],
    message: "The executable is still in use. Try again in a moment.",
  },
  {
    needles: ["exec format error", "os error 8", "enoexec", "not executable"],
    message:
      "That file can't be run directly. Choose an app, script, or executable.",
  },
  {
    needles: [
      "address already in use",
      "os error 48",
      "os error 98",
      "eaddrinuse",
    ],
    message:
      "That port is already in use. Stop the other process or choose a different port.",
  },
  {
    needles: [
      "connection refused",
      "os error 61",
      "os error 111",
      "econnrefused",
    ],
    message: "The service refused the connection. It may still be starting.",
  },
  {
    needles: ["connection reset", "econnreset"],
    message: "The connection closed unexpectedly. Try again.",
  },
  {
    needles: ["timed out", "timeout", "etimedout"],
    message: "The operation timed out. Try again.",
  },
  {
    needles: ["no space left", "enospc", "os error 28"],
    message: "There isn't enough disk space to finish that.",
  },
  {
    needles: [
      "is a directory",
      "not a directory",
      "os error 21",
      "os error 20",
      "eisdir",
      "enotdir",
    ],
    message:
      "A path points to the wrong kind of file. Check the executable and working directory.",
  },
  {
    needles: ["too many open files", "emfile"],
    message: "ProcessHub has too many files open. Restart it and try again.",
  },
  {
    needles: ["database is locked", "sqlite_busy"],
    message: "ProcessHub is busy saving changes. Try again.",
  },
  {
    needles: ["database", "sqlite", "rusqlite"],
    message: "ProcessHub couldn't save that change. Try again.",
  },
  {
    needles: ["keychain", "keyring", "secret service", "os error -25300"],
    message: "The secret couldn't be saved to the system keychain. Try again.",
  },
  {
    needles: ["lock poisoned", "poisoned"],
    message: "ProcessHub hit an internal error. Restart the app and try again.",
  },
  {
    needles: ["tcp check", "http check", "health check failed"],
    message:
      "The health check couldn't reach the service. It may still be starting.",
  },
];

const TECHNICAL_MARKERS = [
  "os error",
  "errno",
  "sqlite",
  "rusqlite",
  "keyring",
  "reqwest",
  "`",
  "\n",
  "\r",
  "panicked",
  "unwrap",
  "rust_backtrace",
  " at src/",
  "://",
];

const MAX_FRIENDLY_LENGTH = 220;

function extractMessage(cause: unknown): string {
  if (typeof cause === "string") return cause;
  if (cause instanceof Error) return cause.message;
  if (
    cause &&
    typeof cause === "object" &&
    "message" in cause &&
    typeof (cause as { message?: unknown }).message === "string"
  ) {
    return (cause as { message: string }).message;
  }
  return "";
}

function containsNeedle(message: string, needle: string): boolean {
  const checkBoundary = needle.startsWith("os error ");
  let offset = 0;
  while (offset <= message.length) {
    const index = message.indexOf(needle, offset);
    if (index === -1) return false;
    const end = index + needle.length;
    const next = message[end];
    if (!checkBoundary || next === undefined || !/\d/.test(next)) {
      return true;
    }
    offset = end;
  }
  return false;
}

function hasTechnicalMarker(message: string): boolean {
  const lower = message.toLowerCase();
  return (
    [...message].length > MAX_FRIENDLY_LENGTH ||
    TECHNICAL_MARKERS.some((marker) => lower.includes(marker))
  );
}

export function friendlyError(cause: unknown, fallback: string): string {
  console.warn("ProcessHub error:", cause);

  const message = extractMessage(cause)
    .replace(/^error:\s*/i, "")
    .trim();
  if (!message) return fallback;

  const lower = message.toLowerCase();
  for (const rule of RAW_PATTERNS) {
    if (rule.needles.some((needle) => containsNeedle(lower, needle))) {
      return rule.message;
    }
  }

  return hasTechnicalMarker(message) ? fallback : message;
}

export function friendlyFailures(
  failures: string[],
  action: "start" | "stop" | "restart",
): string {
  const reasons = new Set<string>();
  for (const failure of failures) {
    const reason = friendlyError(failure, "");
    if (reason) reasons.add(reason);
  }

  const detail =
    reasons.values().next().value ?? "Check their logs for details.";
  const noun = failures.length === 1 ? "service" : "services";
  return `${failures.length} ${noun} couldn't ${action}. ${detail}`;
}
