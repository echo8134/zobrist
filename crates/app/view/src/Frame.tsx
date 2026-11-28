import { useCallback, useLayoutEffect, useRef, useState } from "react";
import {
  type DroppedPaths,
  fileOfThePage,
  openPgnPath,
  readFileAgain,
} from "./FilesPage";
import styles from "./Frame.module.css";
import { IconBar } from "./IconBar";
import { ask, finishBoardActions } from "./ipc";
import { PageArea } from "./PageArea";
import { editSetup } from "./positionSetup";
import { TabArea } from "./TabArea";
import { TopBar } from "./TopBar";
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
} from "./tabs";

declare global {
  interface WindowEventMap {
    "zobrist-file-drop": CustomEvent<DroppedPaths>;
  }
}

const { frame, startup, problem: problemLine } = styles;

export function Frame() {
  const [shown, setShown] = useState<Shown>();
  const [problem, setProblem] = useState<string>();
  const [iconBarOpen, setIconBarOpen] = useState(true);
  // A save or a close reads changes that React may not have drawn yet.
  const newest = useRef<Shown>({ tabs: [], page: "Home", shownTab: undefined });
  const change = useCallback((next: Change) => {
    newest.current = apply(newest.current, next);
    setShown(newest.current);
  }, []);
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
    const onDrop = async (event: CustomEvent<DroppedPaths>) => {
      if (!(await restoring.current) || document.body.inert) return;
      change({ type: "showPage", page: "Files" });
      await openPgnPath(event.detail);
    };
    const close = async () => {
      if (document.body.inert) return;
      const focused = document.activeElement;
      if (focused instanceof HTMLElement) focused.blur();
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
          await finishBoardActions();
          const { tabs, shownTab } = newest.current;
          await ask({
            type: "QuitWithBoards",
            boards: tabs.map(({ game }) => game),
            selected: shownTab ?? null,
          });
          return;
        }
      } catch (error) {
        setProblem(`Cannot save your boards before quitting. ${String(error)}`);
      }
      document.body.inert = false;
      input.abort();
    };
    window.addEventListener("zobrist-close-requested", close);
    window.addEventListener("zobrist-file-drop", onDrop);
    restore();
    return () => {
      window.removeEventListener("zobrist-close-requested", close);
      window.removeEventListener("zobrist-file-drop", onDrop);
    };
  }, [change, restore]);

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
      {problem && (
        <p role="alert" className={problemLine}>
          {problem}
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
        <PageArea page={page} change={change} />
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
