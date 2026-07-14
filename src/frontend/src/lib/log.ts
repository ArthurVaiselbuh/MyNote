import { error, info, trace, warn } from "@tauri-apps/plugin-log";

// Frontend logs flow to the same Rust file target and level filter as backend
// logs; a failed IPC (e.g. before the plugin is ready) must never surface in
// the UI, so every call swallows its rejection.
export const log = {
  verbose: (message: string) => void trace(message).catch(() => {}),
  info: (message: string) => void info(message).catch(() => {}),
  warn: (message: string) => void warn(message).catch(() => {}),
  error: (message: string) => void error(message).catch(() => {}),
};
