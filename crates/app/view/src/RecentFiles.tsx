import { useEffect, useState } from "react";
import type { CollectionEntry } from "./generated/CollectionEntry";
import { ask } from "./ipc";
import styles from "./RecentFiles.module.css";

const { recent, path } = styles;
const readers = new Set<() => void>();

// Home can already be visible when a file finishes opening on Files.
export function fileWasOpened() {
  for (const read of readers) read();
}

type Props = { onOpen: (entry: CollectionEntry) => void };

export function RecentFiles({ onOpen }: Props) {
  const [files, setFiles] = useState<CollectionEntry[] | string>();
  useEffect(() => {
    let latest = 0;
    const read = async () => {
      const request = ++latest;
      const result = await ask({ type: "GetRecentFiles" }).then(
        ({ entries }) => entries,
        (reason: unknown) =>
          reason instanceof Error ? reason.message : String(reason),
      );
      if (request === latest) setFiles(result);
    };
    readers.add(read);
    void read();
    return () => {
      ++latest;
      readers.delete(read);
    };
  }, []);

  return (
    <section className={recent} aria-label="Recent files">
      <h2>Recent files</h2>
      {typeof files === "string" ? (
        <p role="alert">{files}</p>
      ) : files?.length === 0 ? (
        <p>No recent files yet.</p>
      ) : files ? (
        <ol>
          {files.map((entry) => (
            <li key={entry.path}>
              <button
                type="button"
                title={entry.path}
                onClick={() => onOpen(entry)}
              >
                <span>{entry.name}</span>
                <span className={path}>{entry.path}</span>
              </button>
            </li>
          ))}
        </ol>
      ) : null}
    </section>
  );
}
