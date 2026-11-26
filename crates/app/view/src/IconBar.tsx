import {
  Calendar,
  Cpu,
  Database,
  Folder,
  House,
  type LucideIcon,
  Settings,
  User,
} from "lucide-react";
import styles from "./IconBar.module.css";
import { lowerPages, type Page, upperPages } from "./PageArea";

const { iconBar, collapsed, group, entry, name } = styles;

const icons: Record<Page, LucideIcon> = {
  Home: House,
  Files: Folder,
  Databases: Database,
  Events: Calendar,
  Engines: Cpu,
  User,
  Settings,
};

type Props = {
  open: boolean;
  currentPage: Page;
  onChoose: (page: Page) => void;
};

export function IconBar({ open, currentPage, onChoose }: Props) {
  const entries = (pages: readonly Page[]) => (
    <div className={group}>
      {pages.map((page) => {
        const Icon = icons[page];
        return (
          <button
            key={page}
            type="button"
            className={entry}
            aria-label={page}
            aria-current={page === currentPage ? "page" : undefined}
            onClick={() => onChoose(page)}
          >
            <Icon />
            <span className={name}>{page}</span>
          </button>
        );
      })}
    </div>
  );
  return (
    <nav className={open ? iconBar : `${iconBar} ${collapsed}`}>
      {entries(upperPages)}
      {entries(lowerPages)}
    </nav>
  );
}
