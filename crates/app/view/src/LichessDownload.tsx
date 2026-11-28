import { useState, useSyncExternalStore } from "react";
import { ask, errorText } from "./ipc";
import styles from "./UserPage.module.css";

const { nameForm, dateRange, actions, refusal } = styles;

type Download = { name: string; busy: boolean; message: string };

// A player can leave User and return while the picker or export is still open.
let current: Download | null = null;
const changes = new EventTarget();
function subscribe(hear: () => void) {
  changes.addEventListener("change", hear);
  return () => changes.removeEventListener("change", hear);
}
function show(download: Download | null) {
  current = download;
  changes.dispatchEvent(new Event("change"));
}

async function download(
  name: string,
  from: string,
  to: string,
  onGamesWritten: (path: string) => void,
) {
  if (current?.busy) return;
  show({ name, busy: true, message: "Choose a PGN file…" });
  try {
    const { path } = await ask({
      type: "PickPgnPath",
      choice: "File",
      folder: null,
    });
    if (path === null) {
      show(null);
      return;
    }
    show({ name, busy: true, message: "Downloading games…" });
    const { added, file, warning } = await ask({
      type: "DownloadLichessGames",
      name,
      from,
      to,
      path,
    });
    const count = `Added ${added} ${added === 1 ? "game" : "games"} to ${file}.`;
    show({
      name,
      busy: false,
      message: warning ? `${count} ${warning}` : count,
    });
    if (added > 0) onGamesWritten(path);
  } catch (error) {
    show({ name, busy: false, message: errorText(error) });
  }
}

export function LichessDownload({
  savedName,
  writing,
  onGamesWritten,
}: {
  savedName: string | null | undefined;
  writing: boolean;
  onGamesWritten: (path: string) => void;
}) {
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const job = useSyncExternalStore(subscribe, () => current);
  const reversed = from !== "" && to !== "" && from > to;
  const disabled =
    !savedName || writing || !from || !to || reversed || job?.busy;
  return (
    <form
      className={nameForm}
      aria-label="Public Lichess games"
      onSubmit={(event) => {
        event.preventDefault();
        if (!disabled && savedName)
          void download(savedName, from, to, onGamesWritten);
      }}
    >
      <div className={dateRange}>
        <label>
          From
          <input
            type="date"
            required
            value={from}
            disabled={job?.busy}
            onChange={(event) => setFrom(event.target.value)}
          />
        </label>
        <label>
          To
          <input
            type="date"
            required
            value={to}
            disabled={job?.busy}
            onChange={(event) => setTo(event.target.value)}
          />
        </label>
      </div>
      <div className={actions}>
        <button type="submit" disabled={disabled}>
          Download games
        </button>
      </div>
      {reversed && (
        <span role="alert" className={refusal}>
          From must be on or before To.
        </span>
      )}
      {job && (
        <span role="status">
          {job.name}: {job.message}
        </span>
      )}
    </form>
  );
}
