import { useReducer, useState } from "react";
import styles from "./Frame.module.css";
import { IconBar } from "./IconBar";
import { type Page, PageArea, TabArea } from "./PageArea";
import { TopBar } from "./TopBar";

const { frame } = styles;

type Shown = {
  tabs: readonly number[];
  nextUnusedTab: number;
  page: Page;
  shownTab: number | undefined;
};

type Change =
  | { type: "openTab" }
  | { type: "showTab"; tab: number }
  | { type: "closeTab"; tab: number }
  | { type: "showPage"; page: Page };

function apply(shown: Shown, change: Change): Shown {
  switch (change.type) {
    case "openTab":
      return {
        ...shown,
        tabs: [...shown.tabs, shown.nextUnusedTab],
        nextUnusedTab: shown.nextUnusedTab + 1,
        shownTab: shown.nextUnusedTab,
      };
    case "showTab":
      return { ...shown, shownTab: change.tab };
    case "closeTab": {
      const place = shown.tabs.indexOf(change.tab);
      const tabAtRight = shown.tabs[place + 1];
      const tabAtLeft = shown.tabs[place - 1];
      return {
        ...shown,
        tabs: shown.tabs.filter((tab) => tab !== change.tab),
        shownTab:
          shown.shownTab === change.tab
            ? (tabAtRight ?? tabAtLeft)
            : shown.shownTab,
      };
    }
    case "showPage":
      return { ...shown, page: change.page, shownTab: undefined };
  }
}

export function Frame() {
  const [{ tabs, page, shownTab }, change] = useReducer(apply, {
    tabs: [],
    nextUnusedTab: 1,
    page: "Home",
    shownTab: undefined,
  });
  const [iconBarOpen, setIconBarOpen] = useState(true);
  return (
    <div className={frame}>
      <TopBar
        iconBarOpen={iconBarOpen}
        onToggleIconBar={() => setIconBarOpen(!iconBarOpen)}
        tabs={tabs}
        shownTab={shownTab}
        onOpenTab={() => change({ type: "openTab" })}
        onShowTab={(tab) => change({ type: "showTab", tab })}
        onCloseTab={(tab) => change({ type: "closeTab", tab })}
      />
      <IconBar
        open={iconBarOpen}
        currentPage={shownTab === undefined ? page : undefined}
        onChoose={(chosen) => change({ type: "showPage", page: chosen })}
      />
      {shownTab === undefined ? <PageArea page={page} /> : <TabArea />}
    </div>
  );
}
