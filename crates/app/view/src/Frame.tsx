import { useReducer, useState } from "react";
import styles from "./Frame.module.css";
import { IconBar } from "./IconBar";
import { PageArea } from "./PageArea";
import { TabArea } from "./TabArea";
import { TopBar } from "./TopBar";
import {
  apply,
  closeTab,
  goTo,
  openTab,
  playMove,
  setResult,
  setShapes,
  setTag,
  tabName,
} from "./tabs";

const { frame } = styles;

export function Frame() {
  const [{ tabs, page, shownTab }, change] = useReducer(apply, {
    tabs: [],
    page: "Home",
    shownTab: undefined,
  });
  const [iconBarOpen, setIconBarOpen] = useState(true);
  const tab = tabs.find(({ game }) => game === shownTab);
  return (
    <div className={frame}>
      <TopBar
        iconBarOpen={iconBarOpen}
        onToggleIconBar={() => setIconBarOpen(!iconBarOpen)}
        tabs={tabs.map(({ game, picture }) => ({
          game,
          name: tabName(picture),
        }))}
        shownTab={shownTab}
        onOpenTab={() => openTab(change)}
        onShowTab={(game) => change({ type: "showTab", game })}
        onCloseTab={(game) => closeTab(game, change)}
      />
      <IconBar
        open={iconBarOpen}
        currentPage={tab === undefined ? page : undefined}
        onChoose={(chosen) => change({ type: "showPage", page: chosen })}
      />
      {tab === undefined ? (
        <PageArea page={page} />
      ) : (
        <TabArea
          key={tab.game}
          tab={tab}
          onTurnBoard={() => change({ type: "turnBoard", game: tab.game })}
          onMove={(move) => playMove(tab.game, move, change)}
          onGoTo={(place) => goTo(tab.game, place, change)}
          onShapes={(shapes) => setShapes(tab.game, shapes, change)}
          onSetTag={(tag, text) => setTag(tab, tag, text, change)}
          onSetResult={(result) => setResult(tab.game, result, change)}
        />
      )}
    </div>
  );
}
