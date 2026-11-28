import { useCallback, useLayoutEffect, useMemo, useRef, useState } from "react";
import { boardSession } from "../board/boardSession";
import { editSetup } from "../board/positionSetup";
import { TabArea } from "../board/TabArea";
import {
  annotate,
  apply,
  askForTool,
  type Change,
  closeOrAsk,
  closeTab,
  goTo,
  openTab,
  playMove,
  type SaveTool,
  type Shown,
  save,
  setResult,
  setShapes,
  setTag,
  tabName,
} from "../board/tabs";
import type { App } from "../component";
import {
  type DroppedPaths,
  fileOfThePage,
  openPgnPath,
  readFileAgain,
} from "../FilesPage";
import { ask } from "../ipc";
import styles from "./Frame.module.css";
import { IconBar } from "./IconBar";
import { PageArea } from "./PageArea";
import { TopBar } from "./TopBar";

declare global {
  interface WindowEventMap {
    "zobrist-file-drop": CustomEvent<DroppedPaths>;
  }
}

const { frame, startup, waiting: waitingLine, problem: problemLine } = styles;

export function Frame() {
  const [shown, setShown] = useState<Shown>();
  const [problem, setProblem] = useState<string>();
  const [waiting, setWaiting] = useState<string | null>(null);
  const [checkpointProblem, setCheckpointProblem] = useState<string>();
  const [iconBarOpen, setIconBarOpen] = useState(true);
  // A save or a close reads changes that React may not have drawn yet.
  const newest = useRef<Shown>({ tabs: [], page: "Home", shownTab: undefined });
  const session = useMemo(
    () => boardSession(() => newest.current, setCheckpointProblem),
    [],
  );
  const change = useCallback(
    (next: Change) => {
      const before = newest.current;
      newest.current = apply(before, next);
      setShown(newest.current);
      session.changed(before, next);
    },
    [session],
  );
  const app = useMemo<App>(
    () => ({
      openOnBoard: ({ game, picture }) =>
        change({ type: "openTab", game, picture }),
    }),
    [change],
  );
  const restoring = useRef<Promise<boolean> | undefined>(undefined);
  const restore = useCallback(() => {
    if (restoring.current !== undefined) return;
    setProblem(undefined);
    restoring.current = ask({ type: "RestoreBoards" }).then(
      (answer) => {
        change(answer);
        setProblem(answer.warning);
        return true;
      },
      (error: unknown) => {
        setProblem(`Cannot reopen your boards. ${String(error)}`);
        restoring.current = undefined;
        return false;
      },
    );
  }, [change]);

  useLayoutEffect(() => {
    let closing: AbortController | undefined;
    const leaveFocusedField = () => {
      const focused = document.activeElement;
      if (focused instanceof HTMLElement) focused.blur();
    };
    const onDrop = async (event: CustomEvent<DroppedPaths>) => {
      if (!(await restoring.current) || document.body.inert) return;
      leaveFocusedField();
      change({ type: "showPage", page: "Files" });
      await openPgnPath(event.detail);
    };
    const close = async () => {
      if (closing) {
        closing.abort();
        return;
      }
      closing = new AbortController();
      leaveFocusedField();
      document.body.inert = true;
      // Document shortcuts and an unfinished drag can still receive events outside the inert body.
      const input = new AbortController();
      for (const type of ["keydown", "mouseup", "touchend"]) {
        window.addEventListener(type, (event) => event.stopPropagation(), {
          capture: true,
          signal: input.signal,
        });
      }
      try {
        if (await restoring.current) {
          await session.quit(closing.signal, setWaiting);
          return;
        }
      } catch (error) {
        setProblem(`Cannot save your boards before quitting. ${String(error)}`);
      }
      setWaiting(null);
      closing = undefined;
      document.body.inert = false;
      input.abort();
    };
    window.addEventListener("zobrist-close-requested", close);
    window.addEventListener("zobrist-file-drop", onDrop);
    restore();
    return () => {
      window.removeEventListener("zobrist-close-requested", close);
      window.removeEventListener("zobrist-file-drop", onDrop);
      session.cancel();
    };
  }, [change, restore, session]);

  if (shown === undefined) {
    return (
      <main className={startup}>
        {problem === undefined ? (
          <p role="status">Opening boards…</p>
        ) : (
          <>
            <p role="alert">{problem}</p>
            <button type="button" onClick={restore}>
              Retry
            </button>
          </>
        )}
      </main>
    );
  }
  const { tabs, page, shownTab, asksToClose } = shown;
  const tab = tabs.find(({ game }) => game === shownTab);

  const tabNow = (game: number) =>
    newest.current.tabs.find((each) => each.game === game);
  const saveGame = async (game: number, tool: SaveTool) => {
    if (await save(game, tool, fileOfThePage(), tabNow, change)) {
      await readFileAgain();
    }
  };

  return (
    <div className={frame}>
      {waiting && (
        <dialog
          className={waitingLine}
          aria-labelledby="closing-message"
          ref={(dialog) => {
            if (dialog && !dialog.open) dialog.showModal();
          }}
          onCancel={(event) => event.preventDefault()}
        >
          <p role="status" id="closing-message">
            {waiting}
          </p>
        </dialog>
      )}
      {(problem || checkpointProblem) && (
        <p role="alert" className={problemLine}>
          {[problem, checkpointProblem].filter(Boolean).join(" ")}
        </p>
      )}
      <TopBar
        iconBarOpen={iconBarOpen}
        onToggleIconBar={() => setIconBarOpen(!iconBarOpen)}
        tabs={tabs.map(({ game, picture }) => ({
          game,
          name: tabName(picture),
          unsaved: picture.unsaved,
        }))}
        shownTab={shownTab}
        asksToClose={asksToClose}
        onOpenTab={() => openTab(change)}
        onShowTab={(game) => change({ type: "showTab", game })}
        onCloseTab={(game) => closeOrAsk(game, tabNow, change)}
        onCloseWithoutSaving={(game) => closeTab(game, change)}
        onKeepTab={() => change({ type: "keepTab" })}
      />
      <IconBar
        open={iconBarOpen}
        currentPage={tab === undefined ? page : undefined}
        onChoose={(chosen) => change({ type: "showPage", page: chosen })}
      />
      {tab === undefined ? (
        <PageArea page={page} change={change} app={app} />
      ) : (
        <TabArea
          key={tab.game}
          tab={tab}
          change={change}
          onEditSetup={(edit) => editSetup(tab.game, edit, tabNow, change)}
          onTurnBoard={() => change({ type: "turnBoard", game: tab.game })}
          onMove={(move) => playMove(tab.game, move, change)}
          onGoTo={(place) => goTo(tab.game, place, change)}
          onTool={(tool) => askForTool(tab.game, tool, change)}
          onSave={(tool) => saveGame(tab.game, tool)}
          onSetGameData={(gameData) =>
            change({ type: "setGameData", game: tab.game, gameData })
          }
          onAnnotate={(annotation) => annotate(tab.game, annotation, change)}
          onShapes={(shapes) => setShapes(tab.game, shapes, change)}
          onSetTag={(tag, text) => setTag(tab, tag, text, change)}
          onSetResult={(result) => setResult(tab.game, result, change)}
        />
      )}
    </div>
  );
}
