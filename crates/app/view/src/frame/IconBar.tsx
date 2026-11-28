import { House, type LucideIcon, Settings } from "lucide-react";
import { components, type PageName } from "../components/list";
import styles from "./IconBar.module.css";

const { iconBar, collapsed, group, entry, name } = styles;

const pages: readonly { page: PageName; icon: LucideIcon }[] = [
  { page: "Home", icon: House },
  ...components.map((listed) => ({
    page: listed.name,
    icon: listed.page.icon,
  })),
  { page: "Settings", icon: Settings },
];
/** The pages whose entries stand at the bottom of the icon bar. */
const lowerGroup: ReadonlySet<PageName> = new Set(["User", "Settings"]);

type Props = {
  open: boolean;
  currentPage: PageName | undefined;
  onChoose: (page: PageName) => void;
};

export function IconBar({ open, currentPage, onChoose }: Props) {
  const entries = (lower: boolean) => (
    <div className={group}>
      {pages
        .filter(({ page }) => lowerGroup.has(page) === lower)
        .map(({ page, icon: Icon }) => (
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
        ))}
    </div>
  );
  return (
    <nav className={open ? iconBar : `${iconBar} ${collapsed}`}>
      {entries(false)}
      {entries(true)}
    </nav>
  );
}
