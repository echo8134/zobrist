import { PanelLeft } from "lucide-react";
import { useState } from "react";
import styles from "./Frame.module.css";
import { IconBar } from "./IconBar";
import { type Page, PageArea } from "./PageArea";

const { frame, topBar, toggle, name } = styles;

export function Frame() {
  const [currentPage, setCurrentPage] = useState<Page>("Home");
  const [iconBarOpen, setIconBarOpen] = useState(true);
  return (
    <div className={frame}>
      <header className={topBar}>
        <button
          type="button"
          className={toggle}
          aria-label={
            iconBarOpen ? "Collapse the icon bar" : "Open the icon bar"
          }
          onClick={() => setIconBarOpen(!iconBarOpen)}
        >
          <PanelLeft />
        </button>
        {iconBarOpen && <span className={name}>zobrist</span>}
      </header>
      <IconBar
        open={iconBarOpen}
        currentPage={currentPage}
        onChoose={setCurrentPage}
      />
      <PageArea page={currentPage} />
    </div>
  );
}
