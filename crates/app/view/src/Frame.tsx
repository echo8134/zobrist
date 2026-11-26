import { useState } from "react";
import styles from "./Frame.module.css";
import { IconBar } from "./IconBar";
import { type Page, PageArea } from "./PageArea";

const { frame, topBar } = styles;

export function Frame() {
  const [currentPage, setCurrentPage] = useState<Page>("Home");
  return (
    <div className={frame}>
      <header className={topBar}>zobrist</header>
      <IconBar currentPage={currentPage} onChoose={setCurrentPage} />
      <PageArea page={currentPage} />
    </div>
  );
}
