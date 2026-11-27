import { once } from "node:events";
import { createServer } from "node:http";
import { text } from "node:stream/consumers";

const portVariable = "ZOBRIST_TEST_EXPORT_PORT";

/** The launcher serves exports to the app. The spec chooses the next reply over loopback. */
export async function startGameExports() {
  let status = 503;
  let pgn = "";
  let requests = 0;
  const server = createServer(async (request, response) => {
    if (request.method === "PUT" && request.url === "/reply") {
      status = Number(request.headers["x-export-status"]);
      pgn = await text(request);
      requests = 0;
      response.writeHead(204).end();
      return;
    }
    if (request.url === "/requests") {
      response.end(String(requests));
      return;
    }

    requests += 1;
    if (
      request.method === "GET" &&
      request.url === "/game/export/Ab12Cd34" &&
      request.headers.accept === "application/x-chess-pgn"
    ) {
      response
        .writeHead(status, { "Content-Type": "application/x-chess-pgn" })
        .end(pgn);
    } else {
      response.writeHead(400).end();
    }
  });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  const address = server.address();
  if (!address || typeof address === "string") {
    throw new Error("The game export fixture needs a TCP port.");
  }
  process.env[portVariable] = String(address.port);
  return server.unref();
}

const endpoint = () => `http://127.0.0.1:${process.env[portVariable]}`;

export async function replyWith(status: number, pgn = "") {
  const reply = await fetch(`${endpoint()}/reply`, {
    method: "PUT",
    headers: { "x-export-status": String(status) },
    body: pgn,
  });
  if (!reply.ok) {
    throw new Error("Could not set the game export fixture.");
  }
}

export async function exportRequests() {
  const reply = await fetch(`${endpoint()}/requests`);
  return Number(await reply.text());
}
