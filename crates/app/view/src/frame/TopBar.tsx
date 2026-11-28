import { PanelLeft, Plus, X } from "lucide-react";
import { useId } from "react";
import { IconButton } from "../controls/IconButton";
import styles from "./TopBar.module.css";

const {
  topBar,
  leftPart,
  collapsed,
  name,
  tabStrip,
  tabList,
  closableTab,
  asking,
  tabButton,
  dot,
  closeButton,
  answers,
} = styles;

const question = "Close without saving?";

type Props = {
  iconBarOpen: boolean;
  onToggleIconBar: () => void;
  tabs: readonly { game: number; name: string; unsaved: boolean }[];
  shownTab: number | undefined;
  /** The tab that asks whether to close it with the changes of its game. */
  asksToClose: number | undefined;
  onOpenTab: () => void;
  onShowTab: (tab: number) => void;
  onCloseTab: (tab: number) => void;
  onCloseWithoutSaving: (tab: number) => void;
  onKeepTab: () => void;
};

export function TopBar({
  iconBarOpen,
  onToggleIconBar,
  tabs,
  shownTab,
  asksToClose,
  onOpenTab,
  onShowTab,
  onCloseTab,
  onCloseWithoutSaving,
  onKeepTab,
}: Props) {
  const questionId = useId();
  return (
    <header className={topBar}>
      <div className={iconBarOpen ? leftPart : `${leftPart} ${collapsed}`}>
        <IconButton
          name={iconBarOpen ? "Collapse the icon bar" : "Open the icon bar"}
          icon={PanelLeft}
          onClick={onToggleIconBar}
        />
        {iconBarOpen && <span className={name}>zobrist</span>}
      </div>
      <div className={tabStrip}>
        {tabs.length > 0 && (
          <div className={tabList} role="tablist">
            {tabs.map(({ game, name: tabName, unsaved }) => {
              const asks = game === asksToClose;
              return (
                <div
                  key={game}
                  className={asks ? `${closableTab} ${asking}` : closableTab}
                >
                  <button
                    type="button"
                    role="tab"
                    className={tabButton}
                    aria-selected={game === shownTab}
                    aria-label={unsaved ? `${tabName}, not saved` : undefined}
                    onClick={() => onShowTab(game)}
                  >
                    {unsaved && <span className={dot} />}
                    {asks ? <span id={questionId}>{question}</span> : tabName}
                  </button>
                  {asks ? (
                    <div className={answers}>
                      <button
                        type="button"
                        aria-describedby={questionId}
                        onClick={() => onCloseWithoutSaving(game)}
                      >
                        Close
                      </button>
                      <button
                        type="button"
                        aria-describedby={questionId}
                        onClick={onKeepTab}
                      >
                        Keep
                      </button>
                    </div>
                  ) : (
                    <button
                      type="button"
                      className={closeButton}
                      aria-label={`Close ${tabName}`}
                      onClick={() => onCloseTab(game)}
                    >
                      <X />
                    </button>
                  )}
                </div>
              );
            })}
          </div>
        )}
        <IconButton name="New board" icon={Plus} onClick={onOpenTab} />
      </div>
    </header>
  );
}
