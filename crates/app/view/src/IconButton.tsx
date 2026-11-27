import type { LucideIcon } from "lucide-react";
import type { MouseEventHandler } from "react";
import styles from "./IconButton.module.css";

const { iconButton } = styles;

type Props = {
  name: string;
  icon: LucideIcon;
  disabled?: boolean;
  pressed?: boolean;
  onClick: MouseEventHandler<HTMLButtonElement>;
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
