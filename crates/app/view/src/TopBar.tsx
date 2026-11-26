import { PanelLeft, Plus, X } from "lucide-react";
import { tabName } from "./PageArea";
import styles from "./TopBar.module.css";

const {
  topBar,
  leftPart,
  collapsed,
  iconButton,
  name,
  tabStrip,
  tabList,
  closableTab,
  tabButton,
  closeButton,
} = styles;

type Props = {
  iconBarOpen: boolean;
  onToggleIconBar: () => void;
  tabs: readonly number[];
  shownTab: number | undefined;
  onOpenTab: () => void;
  onShowTab: (tab: number) => void;
  onCloseTab: (tab: number) => void;
};

export function TopBar({
  iconBarOpen,
  onToggleIconBar,
  tabs,
  shownTab,
  onOpenTab,
  onShowTab,
  onCloseTab,
}: Props) {
  return (
    <header className={topBar}>
      <div className={iconBarOpen ? leftPart : `${leftPart} ${collapsed}`}>
        <button
          type="button"
          className={iconButton}
          aria-label={
            iconBarOpen ? "Collapse the icon bar" : "Open the icon bar"
          }
          onClick={onToggleIconBar}
        >
          <PanelLeft />
        </button>
        {iconBarOpen && <span className={name}>zobrist</span>}
      </div>
      <div className={tabStrip}>
        {tabs.length > 0 && (
          <div className={tabList} role="tablist">
            {tabs.map((tab) => (
              <div key={tab} className={closableTab}>
                <button
                  type="button"
                  role="tab"
                  className={tabButton}
                  aria-selected={tab === shownTab}
                  onClick={() => onShowTab(tab)}
                >
                  {tabName}
                </button>
                <button
                  type="button"
                  className={closeButton}
                  aria-label={`Close ${tabName}`}
                  onClick={() => onCloseTab(tab)}
                >
                  <X />
                </button>
              </div>
            ))}
          </div>
        )}
        <button
          type="button"
          className={iconButton}
          aria-label="New board"
          onClick={onOpenTab}
        >
          <Plus />
        </button>
      </div>
    </header>
  );
}
