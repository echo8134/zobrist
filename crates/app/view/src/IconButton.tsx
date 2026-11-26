import type { LucideIcon } from "lucide-react";
import styles from "./IconButton.module.css";

const { iconButton } = styles;

type Props = {
  name: string;
  icon: LucideIcon;
  onClick: () => void;
};

export function IconButton({ name, icon: Icon, onClick }: Props) {
  return (
    <button
      type="button"
      className={iconButton}
      aria-label={name}
      onClick={onClick}
    >
      <Icon />
    </button>
  );
}
