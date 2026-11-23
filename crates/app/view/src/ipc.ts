import { invoke } from "@tauri-apps/api/core";
import type { Request } from "./generated/Request";
import type { Response } from "./generated/Response";

/** Asks Rust. The answer has the type of the response case that is named like the request. */
export function ask<Asked extends Request>(
  request: Asked,
): Promise<Extract<Response, { type: Asked["type"] }>> {
  return invoke("answer", { request });
}
