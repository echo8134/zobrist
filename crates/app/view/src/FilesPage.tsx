import {
  ChevronDown,
  ChevronRight,
  FileText,
  Folder,
  FolderOpen,
  History,
  Trash2,
} from "lucide-react";
import {
  type ReactNode,
  useEffect,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";
import { EarlierStates, FileConfirmation } from "./EarlierStates";
import styles from "./FilesPage.module.css";
import { GameRows } from "./GameRows";
import type { ChangedGameFile } from "./generated/ChangedGameFile";
import type { CollectionEntry } from "./generated/CollectionEntry";
import type { CollectionRequest } from "./generated/CollectionRequest";
import type { FileCollection } from "./generated/FileCollection";
import type { GameRow } from "./generated/GameRow";
import type { PgnFile } from "./generated/PgnFile";
import type { PgnFileAnswer } from "./generated/PgnFileAnswer";
import type { PickerChoice } from "./generated/PickerChoice";
import type { RefusalReason } from "./generated/RefusalReason";
import { ask, errorText } from "./ipc";
import { fileWasOpened } from "./RecentFiles";

const {
  toolbar,
  button,
  disclosure,
  options,
  workspace,
  collection,
  recovery,
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
  gameActions,
  deleteGame,
} = styles;

type Selection =
  | { kind: "Opening"; path: string }
  | {
      kind: "Unreadable";
      path: string;
      listed: { entry: CollectionEntry; root: string } | null;
    }
  | { kind: "Folder"; entry: CollectionEntry; root: string }
  | { kind: "File"; entry: CollectionEntry; root: string; file: PgnFile };

type Kept = {
  fileCollection: FileCollection;
  expanded: Map<string, CollectionEntry[]>;
  selected: Selection | null;
  selectionError: string | null;
};

// A board can be shown while a file is still being read. Its answer belongs to the
// same Files page when the player comes back.
let kept: Kept = {
  fileCollection: { type: "Ready", entries: [] },
  expanded: new Map(),
  selected: null,
  selectionError: null,
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
  oneFileAtATime: "Drop one PGN file at a time.",
  droppedFolder: "To open a folder, use the Folder… option under Open.",
  notPgn: "Drop a file with the .pgn extension.",
};

function refusalText(answer: Extract<PgnFileAnswer, { type: "Refusal" }>) {
  const reason = refusals[answer.reason];
  return answer.system === null ? reason : `${reason} ${answer.system}`;
}

async function loadCollection(
  type: "GetFileCollection" | "RecoverFileCollection" = "GetFileCollection",
) {
  const request = ++collectionRequest;
  try {
    const { collection } = await ask({ type });
    if (request === collectionRequest) {
      const selectionError =
        collection.type === "Unavailable" &&
        collection.message === kept.selectionError
          ? null
          : kept.selectionError;
      keep({ fileCollection: collection, selectionError });
    }
  } catch (error) {
    if (request === collectionRequest) {
      keep({
        fileCollection: {
          type: "Unavailable",
          message: errorText(error),
          recoverable: false,
        },
      });
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
        keep({ selected: { ...selected, file: answer }, selectionError: null });
      } else {
        keep({
          selected: {
            kind: "Unreadable",
            path: selected.file.path,
            listed: { entry: selected.entry, root: selected.root },
          },
          selectionError: refusalText(answer),
        });
      }
    }
  } catch (error) {
    if (kept.selected === selected && request === refreshRequest) {
      keep({ selectionError: errorText(error) });
    }
  }
}

type SelectionRequest = number | Extract<Selection, { kind: "Opening" }>;

function isCurrentSelection(request: SelectionRequest) {
  // A refused drop leaves an in-flight open in control of its selection.
  return typeof request === "number"
    ? request === selectionRequest
    : request === kept.selected;
}

function beginSelection(path: string) {
  ++selectionRequest;
  const opening = { kind: "Opening", path } as const;
  keep({ selected: opening, selectionError: null });
  return opening;
}

function failedSelection(
  request: SelectionRequest,
  error: unknown,
  previous?: Selection | null,
) {
  if (isCurrentSelection(request)) {
    const fallback = previous?.kind === "Opening" ? null : previous;
    keep({
      selected: fallback === undefined ? kept.selected : fallback,
      selectionError:
        (typeof request !== "number" && kept.selectionError) ||
        errorText(error),
    });
  }
}

function branchContaining(
  path: string,
  entries = kept.fileCollection.type === "Ready"
    ? kept.fileCollection.entries
    : [],
): CollectionEntry | undefined {
  return entries.find((entry) => {
    if (entry.path === path) return true;
    const children = kept.expanded.get(entry.path);
    return children && branchContaining(path, children);
  });
}

export async function openListed(
  entry: CollectionEntry,
  root = branchContaining(entry.path)?.path ?? entry.path,
) {
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
    keep({ expanded, selectionError: null });
    return;
  }

  const request = beginSelection(entry.path);
  try {
    if (entry.kind === "Folder") {
      const { entries } = await ask({
        type: "ReadPgnFolder",
        path: entry.path,
      });
      if (isCurrentSelection(request)) {
        keep({
          expanded: new Map(kept.expanded).set(entry.path, entries),
          selected: { kind: "Folder", entry, root },
        });
      }
    } else {
      const { answer } = await ask({
        type: "OpenListedPgnFile",
        path: entry.path,
      });
      if (answer.type === "Refusal") {
        failedSelection(request, refusalText(answer), {
          kind: "Unreadable",
          path: entry.path,
          listed: { entry, root },
        });
        return;
      }
      fileWasOpened();
      if (isCurrentSelection(request)) {
        keep({ selected: { kind: "File", entry, root, file: answer } });
      }
    }
  } catch (error) {
    failedSelection(request, error, entry.kind === "File" ? null : previous);
  }
}

export type DroppedPaths = Pick<
  Extract<CollectionRequest, { type: "DropPgnPaths" }>,
  "paths"
>;

export async function openPgnPath(
  input: PickerChoice | DroppedPaths,
  chosenPath?: string,
) {
  const previous = kept.selected;
  let fallback: Selection | null | undefined;
  let request: SelectionRequest = selectionRequest;
  try {
    let operation: Extract<
      CollectionRequest,
      { type: "AddPgnPath" | "DropPgnPaths" }
    >;
    if (typeof input === "string") {
      fallback = input === "File" ? null : previous;
      const folder = previous?.kind === "Folder" ? previous.entry.path : null;
      const choice = input;
      const path =
        chosenPath ?? (await ask({ type: "PickPgnPath", choice, folder })).path;
      if (path === null) return;
      if (request === selectionRequest) request = beginSelection(path);
      operation = { type: "AddPgnPath", choice, path, folder };
    } else {
      request = ++selectionRequest;
      ++refreshRequest;
      keep({ selectionError: null });
      operation = { type: "DropPgnPaths", paths: input.paths };
    }
    const { answer } = await ask(operation);
    if (input !== "New" && answer.type === "File") fileWasOpened();
    if (answer.type === "Refusal") {
      if (operation.type === "AddPgnPath" && operation.choice === "File") {
        fallback = { kind: "Unreadable", path: operation.path, listed: null };
      }
      failedSelection(request, refusalText(answer), fallback);
    } else if (isCurrentSelection(request)) {
      const { entry } = answer;
      const fileCollection: FileCollection = {
        type: "Ready",
        entries: answer.entries,
      };
      const expanded = new Map(kept.expanded);
      let selected: Selection;
      if (answer.type === "Folder") {
        expanded.set(entry.path, answer.children);
        selected = { kind: "Folder", entry, root: entry.path };
      } else {
        if (answer.parent !== null)
          expanded.set(answer.parent.path, answer.parent.entries);
        const root =
          answer.parent !== null && previous?.kind === "Folder"
            ? previous.root
            : entry.path;
        selected = { kind: "File", entry, root, file: answer.file };
      }
      keep({
        fileCollection,
        expanded,
        selected,
        selectionError:
          typeof request === "number" ? null : kept.selectionError,
      });
    }
  } catch (error) {
    failedSelection(request, error, fallback);
  }
  // A failed open can be the first sign that the collection on disk needs recovery.
  await loadCollection();
}

async function rereadAfterRestore(path: string) {
  const { selected } = kept;
  if (selected?.kind === "File" && selected.file.path === path) {
    await readFileAgain();
  } else if (selected?.kind === "Unreadable" && selected.path === path) {
    if (selected.listed) {
      await openListed(selected.listed.entry, selected.listed.root);
    } else {
      await openPgnPath("File", path);
    }
  }
}

type Props = {
  onOpenGame: (path: string, number: number) => Promise<boolean>;
  onGameFileChanged: (tab: ChangedGameFile) => void;
};

type DeleteChoice = {
  path: string;
  version: string;
  row: GameRow;
  selectionRequest: number;
};

export function FilesPage({ onOpenGame, onGameFileChanged }: Props) {
  const { fileCollection, expanded, selected, selectionError } =
    useSyncExternalStore(subscribe, () => kept);
  const [choosing, setChoosing] = useState(false);
  const [historyPath, setHistoryPath] = useState<string | null>(null);
  const [recovering, setRecovering] = useState(false);
  const [deletion, setDeletion] = useState<DeleteChoice | "pending" | null>(
    null,
  );
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
  const confirmDelete = async (choice: DeleteChoice) => {
    setDeletion("pending");
    try {
      const { answer } = await ask({
        type: "DeletePgnGame",
        path: choice.path,
        number: choice.row.number,
        version: choice.version,
      });
      if (answer.type === "Refusal") throw new Error(answer.message);
      for (const tab of answer.tabs) onGameFileChanged(tab);
      if (fileOfThePage() === choice.path) await readFileAgain();
    } catch (error) {
      if (
        choice.selectionRequest === selectionRequest &&
        fileOfThePage() === choice.path
      ) {
        keep({ selectionError: errorText(error) });
      }
    } finally {
      setDeletion(null);
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
  const version = file?.version;
  const historyFile =
    file ?? (selected?.kind === "Unreadable" ? selected : null);
  const historyOpen = historyFile !== null && historyPath === historyFile.path;
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
                  void openPgnPath("File");
                }}
              >
                <FileText aria-hidden="true" />
                PGN file…
              </button>
              <button
                type="button"
                onClick={() => {
                  setChoosing(false);
                  void openPgnPath("Folder");
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
          onClick={() => void openPgnPath("New")}
        >
          New PGN…
        </button>
        {historyFile && (
          <button
            type="button"
            className={button}
            aria-expanded={historyOpen}
            onClick={() =>
              setHistoryPath(historyOpen ? null : historyFile.path)
            }
          >
            <History aria-hidden="true" />
            Earlier states
          </button>
        )}
      </div>
      <div className={workspace}>
        <section className={collection} aria-label="Collection">
          <h2 className={caption}>Collection</h2>
          {fileCollection.type === "Unavailable" ? (
            <div className={recovery}>
              <p className={`${line} ${refusal}`} role="status">
                {fileCollection.message}
              </p>
              {fileCollection.recoverable && (
                <button
                  className={button}
                  type="button"
                  disabled={recovering}
                  onClick={async () => {
                    setRecovering(true);
                    await loadCollection("RecoverFileCollection");
                    setRecovering(false);
                  }}
                >
                  Recover file list
                </button>
              )}
            </div>
          ) : fileCollection.entries.length > 0 ? (
            entries(fileCollection.entries)
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
          {selectionError && (
            <p className={`${line} ${refusal}`} role="status">
              {selectionError}
            </p>
          )}
          {selected?.kind === "Opening" && (
            <p className={pathText} role="status">
              Opening {selected.path}…
            </p>
          )}
          {selected?.kind === "Unreadable" && (
            <p className={pathText}>{selected.path}</p>
          )}
          {historyOpen && (
            <EarlierStates
              key={historyFile.path}
              file={historyFile}
              onRestored={rereadAfterRestore}
            />
          )}
          {!selected && !selectionError && fileCollection.type === "Ready" && (
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
                <GameRows
                  rows={file.rows}
                  open={(number) => void open(file.path, number)}
                  renderEvent={(row) => (
                    <div className={gameActions}>
                      <span>{row.event}</span>
                      {version !== undefined && (
                        <button
                          type="button"
                          className={deleteGame}
                          aria-label={`Delete game ${row.number}`}
                          title={`Delete game ${row.number}`}
                          disabled={deletion === "pending"}
                          onClick={(event) => {
                            event.stopPropagation();
                            setDeletion({
                              path: file.path,
                              version,
                              row,
                              selectionRequest,
                            });
                          }}
                        >
                          <Trash2 aria-hidden="true" />
                        </button>
                      )}
                    </div>
                  )}
                />
              </table>
              {deletion &&
                deletion !== "pending" &&
                deletion.selectionRequest === selectionRequest && (
                  <FileConfirmation
                    question={`Delete game ${deletion.row.number} (${deletion.row.white} - ${deletion.row.black}) from ${deletion.path}?`}
                    action="Delete"
                    onCancel={() => setDeletion(null)}
                    onConfirm={() => void confirmDelete(deletion)}
                  />
                )}
            </>
          )}
        </section>
      </div>
    </>
  );
}
