import {
  ArrowUp,
  ChevronDown,
  ChevronUp,
  type LucideIcon,
  Redo,
  Save,
  SavePlus,
  Scissors,
  Trash2,
  Undo,
} from "lucide-react";
import type { GamePicture } from "./generated/GamePicture";
import { IconButton } from "./IconButton";
import styles from "./NotationTools.module.css";
import type { NotationTool, SaveTool } from "./tabs";

const { notationTools, group } = styles;

type Button = {
  tool: NotationTool;
  name: string;
  icon: LucideIcon;
  hasWork: keyof Pick<
    GamePicture,
    "canUndo" | "canRedo" | "inVariation" | "hasNextMove"
  >;
};

const groups: readonly (readonly Button[])[] = [
  [
    { tool: "Undo", name: "Undo", icon: Undo, hasWork: "canUndo" },
    { tool: "Redo", name: "Redo", icon: Redo, hasWork: "canRedo" },
  ],
  [
    {
      tool: "PromoteVariation",
      name: "Promote variation",
      icon: ArrowUp,
      hasWork: "inVariation",
    },
    {
      tool: "DeleteVariation",
      name: "Delete variation",
      icon: Trash2,
      hasWork: "inVariation",
    },
    {
      tool: "DeleteRest",
      name: "Delete the rest",
      icon: Scissors,
      hasWork: "hasNextMove",
    },
  ],
];

type Props = {
  picture: GamePicture;
  onUse: (tool: NotationTool) => void;
  onSave: (tool: SaveTool) => void;
  gameDataOpen: boolean;
  onToggleGameData: () => void;
};

export function NotationTools({
  picture,
  onUse,
  onSave,
  gameDataOpen,
  onToggleGameData,
}: Props) {
  const { file, unsaved } = picture;
  return (
    <div className={notationTools}>
      <div className={group}>
        <IconButton
          name="Save"
          icon={Save}
          disabled={!unsaved}
          onClick={() => onSave("Save")}
        />
        <IconButton
          name="Save as new game"
          icon={SavePlus}
          // A game without a file and without changes is empty.
          disabled={file === null && !unsaved}
          onClick={() => onSave("SaveAsNewGame")}
        />
      </div>
      {groups.map((buttons) => (
        <div key={buttons[0]?.tool} className={group}>
          {buttons.map(({ tool, name, icon, hasWork }) => (
            <IconButton
              key={tool}
              name={name}
              icon={icon}
              disabled={!picture[hasWork]}
              onClick={() => onUse(tool)}
            />
          ))}
        </div>
      ))}
      <IconButton
        name={gameDataOpen ? "Hide the game data" : "Show the game data"}
        icon={gameDataOpen ? ChevronUp : ChevronDown}
        pressed={gameDataOpen}
        onClick={onToggleGameData}
      />
    </div>
  );
}
