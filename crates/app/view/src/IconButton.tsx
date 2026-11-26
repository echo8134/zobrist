import type { LucideIcon } from "lucide-react";
import styles from "./IconButton.module.css";

const { iconButton } = styles;

type Props = {
  name: string;
  icon: LucideIcon;
  disabled?: boolean;
  onClick: () => void;
};

export function IconButton({
  name,
  icon: Icon,
  disabled = false,
  onClick,
}: Props) {
  return (
    <button
      type="button"
      className={iconButton}
      aria-label={name}
      disabled={disabled}
      onClick={onClick}
    >
      <Icon />
    </button>
  );
}
