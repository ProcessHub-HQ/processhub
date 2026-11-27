import type { ReactNode } from "react";
import { TopBar } from "../app/TopBar";

interface AppShellProps {
  children: ReactNode;
}

export function AppShell({ children }: AppShellProps) {
  return (
    <main className="desktop">
      <TopBar />
      <section className="workspace" aria-label="Process workspace">
        {children}
      </section>
    </main>
  );
}
