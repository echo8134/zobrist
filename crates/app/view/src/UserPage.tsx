import { ChessKnight, ChessPawn, ChevronDown, Monitor } from "lucide-react";
import styles from "./UserPage.module.css";

const {
  content,
  profile,
  profileIcon,
  title,
  secondary,
  accounts,
  list,
  provider,
  row,
  piece,
  name: providerName,
  status,
  chevron,
  description: providerDescription,
} = styles;

const providers = [
  {
    name: "Lichess",
    icon: ChessKnight,
    description:
      "Once linking is available, you can sign in to Lichess from your browser to open your bookmarked games.",
  },
  {
    name: "Chess.com",
    icon: ChessPawn,
    description:
      "You can already download your games as PGN from Chess.com and open them in Files.",
  },
];

export function UserPage() {
  return (
    <div className={content}>
      <div className={profile}>
        <span className={profileIcon}>
          <Monitor aria-hidden="true" />
        </span>
        <div>
          <h2 className={title}>Local profile</h2>
          <p className={secondary}>On this device</p>
        </div>
      </div>
      <section aria-labelledby="chess-accounts" className={accounts}>
        <h2 className={title} id="chess-accounts">
          Chess accounts
        </h2>
        <p className={secondary}>Account linking is not available yet.</p>
        <div className={list}>
          {providers.map(({ name, icon: Icon, description }) => (
            <details key={name} className={provider}>
              <summary className={row}>
                <span className={piece}>
                  <Icon aria-hidden="true" />
                </span>
                <span className={providerName}>{name}</span>
                <span className={status}>Not connected</span>
                <ChevronDown aria-hidden="true" className={chevron} />
              </summary>
              <p className={providerDescription}>{description}</p>
            </details>
          ))}
        </div>
      </section>
    </div>
  );
}
