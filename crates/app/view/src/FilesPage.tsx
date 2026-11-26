import { useSyncExternalStore } from "react";
import styles from "./FilesPage.module.css";
import type { PgnFileAnswer } from "./generated/PgnFileAnswer";
import type { RefusalReason } from "./generated/RefusalReason";
import { ask } from "./ipc";

const {
  pathRow,
  field,
  button,
  line,
  refusal,
  list,
  numberColumn,
  resultColumn,
  dateColumn,
} = styles;

type Kept = {
  /** The text in the field. */
  typed: string;
  /** What Rust answered last: a file or a refusal. Nothing before the first answer. */
  answer: PgnFileAnswer | undefined;
};

// The page leaves the window while another page or a tab is shown. Its file stays here, and
// the page that is in the window hears of every change, also of an answer to a request that
// an earlier page sent.
let kept: Kept = { typed: "", answer: undefined };
const pagesInTheWindow = new Set<() => void>();

function keep(next: Kept) {
  kept = next;
  for (const hear of pagesInTheWindow) {
    hear();
  }
}

function subscribe(hear: () => void) {
  pagesInTheWindow.add(hear);
  return () => {
    pagesInTheWindow.delete(hear);
  };
}

async function send(type: "OpenPgnFile" | "CreatePgnFile") {
  const response = await ask({ type, path: kept.typed });
  keep({ ...kept, answer: response.answer });
}

/** The path of the file that the page shows, when it shows one. */
export function fileOfThePage() {
  return kept.answer?.type === "File" ? kept.answer.path : undefined;
}

/**
 * Reads the file of the page again. A save of a tab changes the file, and a game of the list
 * that does not open means that the file is no longer what the list shows.
 */
export async function readFileAgain() {
  const path = fileOfThePage();
  if (path === undefined) {
    return;
  }
  const { answer } = await ask({ type: "OpenPgnFile", path });
  // The player may have opened another file meanwhile.
  if (fileOfThePage() === path) {
    keep({ ...kept, answer });
  }
}

const refusals: Record<RefusalReason, string> = {
  cannotRead: "This file cannot be opened.",
  existsAlready: "This file exists already.",
  cannotCreate: "This file cannot be created.",
};

function lineOf(answer: PgnFileAnswer): string | undefined {
  if (answer.type === "Refusal") {
    return answer.system === null
      ? refusals[answer.reason]
      : `${refusals[answer.reason]} ${answer.system}`;
  }
  if (answer.rows.length === 0) {
    return "The file has no games.";
  }
  if (answer.problems === 0) {
    return undefined;
  }
  return answer.problems === 1
    ? "1 game has a problem."
    : `${answer.problems} games have a problem.`;
}

type Props = {
  /** Opens a game of a file in a tab and tells whether Rust opened it. */
  onOpenGame: (path: string, number: number) => Promise<boolean>;
};

export function FilesPage({ onOpenGame }: Props) {
  const { typed, answer } = useSyncExternalStore(subscribe, () => kept);
  const text = answer === undefined ? undefined : lineOf(answer);
  const open = async (path: string, number: number) => {
    if (!(await onOpenGame(path, number))) {
      await readFileAgain();
    }
  };
  return (
    <>
      <div className={pathRow}>
        <input
          className={field}
          type="text"
          aria-label="Path of a PGN file"
          autoComplete="off"
          spellCheck={false}
          value={typed}
          onChange={(event) => keep({ ...kept, typed: event.target.value })}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              void send("OpenPgnFile");
            }
          }}
        />
        <button
          className={button}
          type="button"
          onClick={() => void send("OpenPgnFile")}
        >
          Open
        </button>
        <button
          className={button}
          type="button"
          onClick={() => void send("CreatePgnFile")}
        >
          Create
        </button>
      </div>
      {text !== undefined && (
        <p
          className={answer?.type === "Refusal" ? `${line} ${refusal}` : line}
          role="status"
        >
          {text}
        </p>
      )}
      {answer?.type === "File" && answer.rows.length > 0 && (
        <table className={list}>
          <thead>
            <tr>
              <th className={numberColumn} scope="col">
                #
              </th>
              <th scope="col">White</th>
              <th scope="col">Black</th>
              <th className={resultColumn} scope="col">
                Result
              </th>
              <th className={dateColumn} scope="col">
                Date
              </th>
              <th scope="col">Event</th>
            </tr>
          </thead>
          <tbody>
            {answer.rows.map((row) => (
              <tr
                key={row.number}
                onClick={() => void open(answer.path, row.number)}
              >
                <td>{row.number}</td>
                <td>{row.white}</td>
                <td>{row.black}</td>
                <td>{row.result}</td>
                <td>{row.date}</td>
                <td>{row.event}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </>
  );
}
