import { PanelLeft, Plus, X } from "lucide-react";
import { IconButton } from "./IconButton";
import styles from "./TopBar.module.css";
import { tabName } from "./tabs";

const {
  topBar,
  leftPart,
  collapsed,
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
        <IconButton name="New board" icon={Plus} onClick={onOpenTab} />
      </div>
    </header>
  );
}
