import type { LucideIcon } from "lucide-react";
import styles from "./IconButton.module.css";

const { iconButton } = styles;

type Props = {
  name: string;
  icon: LucideIcon;
  disabled?: boolean;
  pressed?: boolean;
  onClick: () => void;
};

export function IconButton({
  name,
  icon: Icon,
  disabled = false,
  pressed,
  onClick,
}: Props) {
  return (
    <button
      type="button"
      className={iconButton}
      aria-label={name}
      aria-pressed={pressed}
      disabled={disabled}
      onClick={onClick}
    >
      <Icon />
    </button>
  );
}
