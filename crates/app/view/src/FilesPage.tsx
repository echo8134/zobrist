import {
  ChevronDown,
  ChevronRight,
  FileText,
  Folder,
  FolderOpen,
} from "lucide-react";
import {
  type ReactNode,
  useEffect,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";
import styles from "./FilesPage.module.css";
import type { CollectionEntry } from "./generated/CollectionEntry";
import type { PgnFile } from "./generated/PgnFile";
import type { PgnFileAnswer } from "./generated/PgnFileAnswer";
import type { PickerChoice } from "./generated/PickerChoice";
import type { RefusalReason } from "./generated/RefusalReason";
import { ask } from "./ipc";

const {
  toolbar,
  button,
  disclosure,
  options,
  workspace,
  collection,
  caption,
  tree,
  entryButton,
  details,
  titleRow,
  title,
  count,
  pathText,
  line,
  refusal,
  list,
  numberColumn,
  resultColumn,
  dateColumn,
} = styles;

type Selection =
  | { kind: "Opening"; path: string }
  | { kind: "Folder"; entry: CollectionEntry; root: string }
  | { kind: "File"; entry: CollectionEntry; root: string; file: PgnFile };

type Kept = {
  roots: CollectionEntry[];
  expanded: Map<string, CollectionEntry[]>;
  selected: Selection | null;
  error: string | null;
};

// A board can be shown while a file is still being read. Its answer belongs to the
// same Files page when the player comes back.
let kept: Kept = {
  roots: [],
  expanded: new Map(),
  selected: null,
  error: null,
};
const pagesInTheWindow = new Set<() => void>();
let selectionRequest = 0;
let refreshRequest = 0;
let collectionRequest = 0;

function keep(change: Partial<Kept>) {
  kept = { ...kept, ...change };
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

const refusals: Record<RefusalReason, string> = {
  cannotRead: "This file cannot be opened.",
  existsAlready: "This file exists already.",
  cannotCreate: "This file cannot be created.",
};

function refusalText(answer: Extract<PgnFileAnswer, { type: "Refusal" }>) {
  const reason = refusals[answer.reason];
  return answer.system === null ? reason : `${reason} ${answer.system}`;
}

function errorText(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

async function loadCollection() {
  const request = ++collectionRequest;
  try {
    const { entries } = await ask({ type: "GetFileCollection" });
    if (request === collectionRequest) {
      keep({ roots: entries });
    }
  } catch (error) {
    if (request === collectionRequest) {
      keep({ error: errorText(error) });
    }
  }
}

/** The PGN that the next save from a new tab can use. */
export function fileOfThePage() {
  return kept.selected?.kind === "File" ? kept.selected.file.path : undefined;
}

/** A tab saved a game or tried to open a row that is no longer in the file. */
export async function readFileAgain() {
  const { selected } = kept;
  if (selected?.kind !== "File") {
    return;
  }
  const request = ++refreshRequest;
  try {
    const { answer } = await ask({
      type: "OpenPgnFile",
      path: selected.file.path,
    });
    if (kept.selected === selected && request === refreshRequest) {
      if (answer.type === "File") {
        keep({ selected: { ...selected, file: answer }, error: null });
      } else {
        keep({ selected: null, error: refusalText(answer) });
      }
    }
  } catch (error) {
    if (kept.selected === selected && request === refreshRequest) {
      keep({ error: errorText(error) });
    }
  }
}

function beginSelection(path: string) {
  const request = ++selectionRequest;
  keep({ selected: { kind: "Opening", path }, error: null });
  return request;
}

function failedSelection(
  request: number,
  previous: Selection | null,
  error: unknown,
) {
  if (request === selectionRequest) {
    keep({
      selected: previous?.kind === "Opening" ? null : previous,
      error: errorText(error),
    });
  }
}

async function openListed(entry: CollectionEntry, root: string) {
  const previous = kept.selected;
  if (
    entry.kind === "Folder" &&
    previous?.kind === "Folder" &&
    previous.entry.path === entry.path &&
    previous.root === root &&
    kept.expanded.has(entry.path)
  ) {
    ++selectionRequest;
    const expanded = new Map(kept.expanded);
    expanded.delete(entry.path);
    keep({ expanded, error: null });
    return;
  }

  const request = beginSelection(entry.path);
  try {
    if (entry.kind === "Folder") {
      const { entries } = await ask({
        type: "ReadPgnFolder",
        path: entry.path,
      });
      if (request === selectionRequest) {
        keep({
          expanded: new Map(kept.expanded).set(entry.path, entries),
          selected: { kind: "Folder", entry, root },
        });
      }
    } else {
      const { answer } = await ask({ type: "OpenPgnFile", path: entry.path });
      if (answer.type === "Refusal") throw new Error(refusalText(answer));
      if (request === selectionRequest) {
        keep({ selected: { kind: "File", entry, root, file: answer } });
      }
    }
  } catch (error) {
    failedSelection(request, entry.kind === "File" ? null : previous, error);
  }
}

async function pick(choice: PickerChoice) {
  const previous = kept.selected;
  const folder = previous?.kind === "Folder" ? previous.entry.path : null;
  let request = selectionRequest;
  try {
    const { path } = await ask({ type: "PickPgnPath", choice, folder });
    if (path === null) return;
    if (request === selectionRequest) request = beginSelection(path);

    const { answer } = await ask({ type: "AddPgnPath", choice, path, folder });
    if (answer.type === "Refusal") throw new Error(refusalText(answer));
    if (request === selectionRequest) {
      const { entry, entries: roots } = answer;
      const expanded = new Map(kept.expanded);
      if (answer.type === "Folder") {
        expanded.set(entry.path, answer.children);
        keep({
          roots,
          expanded,
          selected: { kind: "Folder", entry, root: entry.path },
        });
      } else {
        if (answer.parent !== null)
          expanded.set(answer.parent.path, answer.parent.entries);
        const root =
          answer.parent !== null && previous?.kind === "Folder"
            ? previous.root
            : entry.path;
        keep({
          roots,
          expanded,
          selected: { kind: "File", entry, root, file: answer.file },
        });
      }
    }
    // Another confirmed dialog may finish after this one, even if its file is no longer selected.
    await loadCollection();
  } catch (error) {
    failedSelection(request, choice === "File" ? null : previous, error);
  }
}

type Props = {
  onOpenGame: (path: string, number: number) => Promise<boolean>;
};

export function FilesPage({ onOpenGame }: Props) {
  const { roots, expanded, selected, error } = useSyncExternalStore(
    subscribe,
    () => kept,
  );
  const [choosing, setChoosing] = useState(false);
  const openButton = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    void loadCollection();
  }, []);

  const selectedEntry = selected && "entry" in selected ? selected.entry : null;
  const open = async (path: string, number: number) => {
    if (!(await onOpenGame(path, number))) {
      await readFileAgain();
    }
  };
  const entries = (
    items: CollectionEntry[],
    parentRoot?: string,
  ): ReactNode => (
    <ul className={tree}>
      {items.map((entry) => {
        const root = parentRoot ?? entry.path;
        const children = expanded.get(entry.path);
        const isFolder = entry.kind === "Folder";
        const Icon = isFolder ? (children ? FolderOpen : Folder) : FileText;
        return (
          <li key={entry.path}>
            <button
              className={entryButton}
              type="button"
              title={entry.path}
              aria-current={
                selected &&
                "entry" in selected &&
                selected.entry.path === entry.path &&
                selected.root === root
                  ? "true"
                  : undefined
              }
              aria-expanded={isFolder ? children !== undefined : undefined}
              onClick={() => void openListed(entry, root)}
            >
              {isFolder &&
                (children ? (
                  <ChevronDown aria-hidden="true" />
                ) : (
                  <ChevronRight aria-hidden="true" />
                ))}
              <Icon aria-hidden="true" />
              <span>{entry.name}</span>
            </button>
            {children !== undefined &&
              children.length > 0 &&
              entries(children, root)}
          </li>
        );
      })}
    </ul>
  );
  const file = selected?.kind === "File" ? selected.file : undefined;
  return (
    <>
      <div className={toolbar}>
        <fieldset
          className={disclosure}
          aria-label="Open options"
          onBlur={(event) => {
            if (!event.currentTarget.contains(event.relatedTarget))
              setChoosing(false);
          }}
          onKeyDown={(event) => {
            if (event.key === "Escape" && choosing) {
              setChoosing(false);
              openButton.current?.focus();
            }
          }}
        >
          <button
            className={button}
            type="button"
            ref={openButton}
            aria-expanded={choosing}
            onClick={() => setChoosing(!choosing)}
          >
            Open
            <ChevronDown aria-hidden="true" />
          </button>
          {choosing && (
            <div className={options}>
              <button
                type="button"
                onClick={() => {
                  setChoosing(false);
                  void pick("File");
                }}
              >
                <FileText aria-hidden="true" />
                PGN file…
              </button>
              <button
                type="button"
                onClick={() => {
                  setChoosing(false);
                  void pick("Folder");
                }}
              >
                <Folder aria-hidden="true" />
                Folder…
              </button>
            </div>
          )}
        </fieldset>
        <button
          className={button}
          type="button"
          onClick={() => void pick("New")}
        >
          New PGN…
        </button>
      </div>
      <div className={workspace}>
        <section className={collection} aria-label="Collection">
          <h2 className={caption}>Collection</h2>
          {roots.length > 0 ? (
            entries(roots)
          ) : (
            <p className={line}>No files or folders yet.</p>
          )}
        </section>
        <section className={details} aria-label="Selection">
          {selectedEntry && (
            <>
              <div className={titleRow}>
                <h2 className={title}>{selectedEntry.name}</h2>
                {file && (
                  <span className={count}>
                    {file.rows.length}{" "}
                    {file.rows.length === 1 ? "game" : "games"}
                  </span>
                )}
              </div>
              <p className={pathText}>{selectedEntry.path}</p>
            </>
          )}
          {error && (
            <p className={`${line} ${refusal}`} role="status">
              {error}
            </p>
          )}
          {selected?.kind === "Opening" && (
            <p className={pathText} role="status">
              Opening {selected.path}…
            </p>
          )}
          {!selected && !error && (
            <p className={line}>Select a file to see its games.</p>
          )}
          {selected?.kind === "Folder" &&
            expanded.get(selected.entry.path)?.length === 0 && (
              <p className={line}>No PGN files or folders here.</p>
            )}
          {file && (
            <>
              {file.problems > 0 && (
                <p className={line} role="status">
                  {file.problems === 1
                    ? "1 game has a problem."
                    : `${file.problems} games have a problem.`}
                </p>
              )}
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
                  {file.rows.map((row) => (
                    <tr
                      key={row.number}
                      onClick={() => void open(file.path, row.number)}
                    >
                      <td>
                        <button
                          type="button"
                          aria-label={`Open game ${row.number}`}
                        >
                          {row.number}
                        </button>
                      </td>
                      <td>{row.white}</td>
                      <td>{row.black}</td>
                      <td>{row.result}</td>
                      <td>{row.date}</td>
                      <td>{row.event}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </>
          )}
        </section>
      </div>
    </>
  );
}
