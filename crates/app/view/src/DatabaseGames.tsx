import { useEffect, useRef, useState } from "react";
import { GameRows } from "./controls/GameRows";
import styles from "./DatabaseGames.module.css";
import files from "./FilesPage.module.css";
import type { DatabaseGameQuery } from "./generated/DatabaseGameQuery";
import type { Response } from "./generated/Response";
import { ask } from "./ipc";

const { filters, paging, sort } = styles;
const { numberColumn, resultColumn, dateColumn, button, count, refusal, list } =
  files;

const columns = [
  ["Number", numberColumn],
  ["White"],
  ["Black"],
  ["Result", resultColumn],
  ["Date", dateColumn],
  ["Event"],
] as const;
const textFilters = [
  ["player", "Player"],
  ["event", "Event"],
  ["year", "Year"],
] as const;
const emptyFilters = { player: null, event: null, year: null, result: null };
const firstPage: DatabaseGameQuery = {
  ...emptyFilters,
  sort: "Number",
  descending: false,
  page: 1,
};
const pageSize = 100;

type Page = Extract<Response, { type: "ListDatabaseGames" }>;
type Props = { path: string; open: (number: number) => void };

export function DatabaseGames({ path, open }: Props) {
  const [query, setQuery] = useState(firstPage);
  const [answer, setAnswer] = useState<{
    query: DatabaseGameQuery;
    page: Page | string;
  } | null>(null);
  const pending = useRef(Promise.resolve());

  useEffect(() => {
    let current = true;
    // A slower collection scan may outlive several keystrokes. Only start the latest waiting scan.
    pending.current = pending.current.then(async () => {
      if (!current) return;
      try {
        const page = await ask({ type: "ListDatabaseGames", path, query });
        if (current) setAnswer({ query, page });
      } catch (error) {
        if (current) setAnswer({ query, page: String(error) });
      }
    });
    return () => {
      current = false;
    };
  }, [path, query]);

  function filter(key: keyof typeof emptyFilters, value: string) {
    setQuery({ ...query, [key]: value || null, page: 1 });
  }

  const filtered = query.player || query.event || query.year || query.result;
  const result = answer?.query === query ? answer.page : null;
  const page = typeof result === "string" ? null : result;
  const hasPages =
    query.page > 1 ||
    (answer && typeof answer.page !== "string" && answer.page.total > pageSize);

  let status = "";
  if (typeof result === "string") {
    status = result;
  } else if (!page) {
    status = "Loading games…";
  } else if (page.rows.length === 0) {
    status = "No matching games";
  } else if (hasPages) {
    const first = (query.page - 1) * pageSize + 1;
    status = `${first}–${first + page.rows.length - 1} of ${page.total}`;
  } else if (filtered) {
    status = `${page.total} ${page.total === 1 ? "match" : "matches"}`;
  }

  return (
    <>
      <div className={filters}>
        {textFilters.map(([key, label]) => (
          <label key={key}>
            {label}
            <input
              type="text"
              size={key === "year" ? 4 : 14}
              inputMode={key === "year" ? "numeric" : "text"}
              maxLength={key === "year" ? 4 : undefined}
              value={query[key] ?? ""}
              onChange={(event) => filter(key, event.target.value)}
            />
          </label>
        ))}
        <label>
          Result
          <select
            value={query.result ?? ""}
            onChange={(event) => filter("result", event.target.value)}
          >
            <option value="">All results</option>
            {["1-0", "0-1", "1/2-1/2", "*"].map((value) => (
              <option key={value}>{value}</option>
            ))}
          </select>
        </label>
        <button
          type="button"
          className={button}
          disabled={!filtered}
          onClick={() => setQuery({ ...query, ...emptyFilters, page: 1 })}
        >
          Clear filters
        </button>
      </div>
      <div className={paging}>
        <p
          aria-live="polite"
          className={typeof result === "string" ? refusal : count}
        >
          {status}
        </p>
        {hasPages && (
          <>
            <button
              type="button"
              className={button}
              disabled={!page || query.page === 1}
              onClick={() => setQuery({ ...query, page: query.page - 1 })}
            >
              Previous
            </button>
            <button
              type="button"
              className={button}
              disabled={!page || query.page * pageSize >= page.total}
              onClick={() => setQuery({ ...query, page: query.page + 1 })}
            >
              Next
            </button>
          </>
        )}
      </div>
      <table className={list} aria-label="Collection games">
        <thead>
          <tr>
            {columns.map(([column, width]) => {
              const sorted = query.sort === column;
              return (
                <th
                  key={column}
                  className={width}
                  scope="col"
                  aria-sort={
                    sorted
                      ? query.descending
                        ? "descending"
                        : "ascending"
                      : undefined
                  }
                >
                  <button
                    type="button"
                    className={sort}
                    aria-label={column}
                    onClick={() =>
                      setQuery({
                        ...query,
                        sort: column,
                        descending: sorted && !query.descending,
                        page: 1,
                      })
                    }
                  >
                    {column === "Number" ? "#" : column}
                    {sorted && (
                      <span aria-hidden="true">
                        {query.descending ? "▾" : "▴"}
                      </span>
                    )}
                  </button>
                </th>
              );
            })}
          </tr>
        </thead>
        <GameRows rows={page?.rows ?? []} open={open} />
      </table>
    </>
  );
}
