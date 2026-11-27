import { once } from "node:events";
import { createServer } from "node:http";
import { text } from "node:stream/consumers";

const portVariable = "ZOBRIST_TEST_EXPORT_PORT";

type PlayerReply = "games" | "empty" | "refused";

const playerGames = [
  {
    createdAt: Date.parse("2026-02-01T00:00:00.000Z"),
    pgn: '[Event "February 1"]\n[Date "2026.02.01"]\n[White "RookExample"]\n[Black "Cedar"]\n[Result "1-0"]\n\n1. c4 e5 1-0',
  },
  {
    createdAt: Date.parse("2026-01-31T23:59:59.999Z"),
    pgn: '[Event "January 31"]\n[Date "2026.01.31"]\n[White "Birch"]\n[Black "RookExample"]\n[Result "0-1"]\n\n1. d4 d5 0-1',
  },
  {
    createdAt: Date.parse("2026-01-01T00:00:00.000Z"),
    pgn: '[Event "January 1"]\n[Date "2026.01.01"]\n[White "RookExample"]\n[Black "Aster"]\n[Result "1-0"]\n\n1. e4 e5 1-0',
  },
];

/** The launcher serves exports to the app. The spec chooses the next reply over loopback. */
export async function startGameExports() {
  let status = 503;
  let pgn = "";
  let playerReply = "refused";
  const requests: string[] = [];
  const server = createServer(async (request, response) => {
    if (request.method === "PUT") {
      if (request.url === "/reply") {
        status = Number(request.headers["x-export-status"]);
        pgn = await text(request);
      } else if (request.url === "/player-reply") {
        playerReply = await text(request);
      } else {
        response.writeHead(404).end();
        return;
      }
      requests.length = 0;
      response.writeHead(204).end();
      return;
    }
    if (request.url === "/requests") {
      response.end(requests.join("\n"));
      return;
    }

    const path = request.url ?? "/";
    const url = new URL(path, "http://127.0.0.1");
    requests.push(path);
    if (
      request.method === "GET" &&
      request.url === "/game/export/Ab12Cd34" &&
      request.headers.accept === "application/x-chess-pgn"
    ) {
      response
        .writeHead(status, { "Content-Type": "application/x-chess-pgn" })
        .end(pgn);
    } else if (
      request.method === "GET" &&
      url.pathname === "/api/games/user/RookExample" &&
      request.headers.accept === "application/x-ndjson" &&
      url.searchParams.get("pgnInJson") === "true"
    ) {
      if (playerReply === "refused") {
        response.writeHead(404).end();
        return;
      }
      response.writeHead(200, { "Content-Type": "application/x-ndjson" });
      if (playerReply === "games") {
        const since = Number(url.searchParams.get("since"));
        const until = Number(url.searchParams.get("until"));
        for (const game of playerGames) {
          if (game.createdAt >= since && game.createdAt < until) {
            response.write(`${JSON.stringify(game)}\n`);
          }
        }
      }
      response.end();
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

async function setReply(
  path: string,
  body: string,
  headers: Record<string, string> = {},
) {
  const reply = await fetch(`${endpoint()}${path}`, {
    method: "PUT",
    headers,
    body,
  });
  if (!reply.ok) {
    throw new Error("Could not set the game export fixture.");
  }
}

export function replyWith(status: number, pgn = "") {
  return setReply("/reply", pgn, { "x-export-status": String(status) });
}

export function replyWithPlayerGames(reply: PlayerReply) {
  return setReply("/player-reply", reply);
}

export async function exportPaths(): Promise<string[]> {
  const response = await fetch(`${endpoint()}/requests`);
  const paths = await response.text();
  return paths === "" ? [] : paths.split("\n");
}

export async function exportRequests() {
  return (await exportPaths()).length;
}
