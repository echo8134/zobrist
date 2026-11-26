import {
  Cpu,
  Database,
  Folder,
  House,
  type LucideIcon,
  Settings,
} from "lucide-react";
import styles from "./IconBar.module.css";
import { lowerPages, type Page, upperPages } from "./PageArea";

const { iconBar, group, entry } = styles;

const icons: Record<Page, LucideIcon> = {
  Home: House,
  Files: Folder,
  Databases: Database,
  Engines: Cpu,
  Settings,
};

type Props = {
  currentPage: Page;
  onChoose: (page: Page) => void;
};

export function IconBar({ currentPage, onChoose }: Props) {
  const entries = (pages: readonly Page[]) => (
    <div className={group}>
      {pages.map((page) => {
        const Icon = icons[page];
        return (
          <button
            key={page}
            type="button"
            className={entry}
            aria-current={page === currentPage ? "page" : undefined}
            onClick={() => onChoose(page)}
          >
            <Icon />
            {page}
          </button>
        );
      })}
    </div>
  );
  return (
    <nav className={iconBar}>
      {entries(upperPages)}
      {entries(lowerPages)}
    </nav>
  );
}
