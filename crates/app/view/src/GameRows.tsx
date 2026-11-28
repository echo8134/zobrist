import type { GameRow } from "./generated/GameRow";

type Props = {
  rows: GameRow[];
  open: (number: number) => void;
};

export function GameRows({ rows, open }: Props) {
  return (
    <tbody>
      {rows.map((row) => (
        <tr key={row.number} onClick={() => open(row.number)}>
          <td>
            <button type="button" aria-label={`Open game ${row.number}`}>
              {row.number}
            </button>
          </td>
          <td>{row.white}</td>
          <td>{row.black}</td>
          <td>{row.result}</td>
          <td>{row.date}</td>
          <td>{row.event}</td>
        </tr>
      ))}
    </tbody>
  );
}
