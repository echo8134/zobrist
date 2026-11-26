import { useRef, useState } from "react";
import { fileOfThePage, readFileAgain } from "./FilesPage";
import styles from "./Frame.module.css";
import { IconBar } from "./IconBar";
import { PageArea } from "./PageArea";
import { TabArea } from "./TabArea";
import { TopBar } from "./TopBar";
import {
  apply,
  askForTool,
  type Change,
  closeOrAsk,
  closeTab,
  goTo,
  openGame,
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

const { frame } = styles;

export function Frame() {
  const [shown, setShown] = useState<Shown>({
    tabs: [],
    page: "Home",
    shownTab: undefined,
  });
  // The state with every change so far, also before React has drawn it. A save and a close
  // read their tab here after the answers to the requests before them.
  const newest = useRef(shown);
  const change = (next: Change) => {
    newest.current = apply(newest.current, next);
    setShown(newest.current);
  };
  const { tabs, page, shownTab, asksToClose } = shown;
  const [iconBarOpen, setIconBarOpen] = useState(true);
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
        <PageArea
          page={page}
          onOpenGame={(path, number) => openGame(path, number, change)}
        />
      ) : (
        <TabArea
          key={tab.game}
          tab={tab}
          onTurnBoard={() => change({ type: "turnBoard", game: tab.game })}
          onMove={(move) => playMove(tab.game, move, change)}
          onGoTo={(place) => goTo(tab.game, place, change)}
          onTool={(tool) => askForTool(tab.game, tool, change)}
          onSave={(tool) => saveGame(tab.game, tool)}
          onShapes={(shapes) => setShapes(tab.game, shapes, change)}
          onSetTag={(tag, text) => setTag(tab, tag, text, change)}
          onSetResult={(result) => setResult(tab.game, result, change)}
        />
      )}
    </div>
  );
}
