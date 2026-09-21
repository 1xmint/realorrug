// SPDX-License-Identifier: Apache-2.0
//! The shell: the header, the footer, and which page is showing.

import { Link, Redirect, Route, Switch, useLocation } from "wouter";

import { About } from "./About";
import { Contact } from "./Contact";
import { account, handleHref } from "./honesty";
import { History } from "./History";
import { Home } from "./Home";
import { HowItWorks } from "./HowItWorks";
import { Check } from "./Check";
import { Privacy } from "./Privacy";
import { footer as footerRoutes, MOVED, nav } from "./routes";
import { Terms } from "./Terms";
import { Token } from "./Token";

function Header() {
  const [location] = useLocation();
  return (
    <header className="sticky top-0 z-30 border-b border-[var(--color-line)] bg-[var(--color-ink)]/85 backdrop-blur">
      <div className="mx-auto flex max-w-5xl items-center justify-between gap-3 px-4 py-3 sm:gap-4 sm:px-6 sm:py-4">
        <Link
          href="/"
          className="display text-lg whitespace-nowrap text-[var(--color-text)] sm:text-xl"
        >
          Real <span className="text-[var(--color-signal)]">or</span> Rug
        </Link>
        <nav className="flex items-center gap-0.5 text-xs sm:gap-1 sm:text-sm">
          {nav()
            .filter((r) => r.path !== "/")
            .map((r) => (
              <Link
                key={r.path}
                href={r.path}
                className={`rounded px-2 py-1.5 whitespace-nowrap transition-colors sm:px-3 ${
                  location === r.path
                    ? "bg-[var(--color-raised)] text-[var(--color-text)]"
                    : "text-[var(--color-dim)] hover:text-[var(--color-text)]"
                }`}
                // Read out by a screen reader, and it is the only thing telling
                // a non-sighted reader which page they are on -- the background
                // colour above says it to everybody else.
                aria-current={location === r.path ? "page" : undefined}
              >
                {r.short ?? r.label}
              </Link>
            ))}
        </nav>
      </div>
    </header>
  );
}

function Footer() {
  const handle = account();
  return (
    <footer className="relative z-10 border-t border-[var(--color-line)]">
      <div className="mx-auto max-w-5xl px-6 py-10 text-sm text-[var(--color-faint)]">
        <p className="max-w-2xl">
          Real or Rug is an automated account. It reports what it measured on
          chain and refuses the rest. <strong>Measured, not predicted.</strong>{" "}
          Nothing here is financial advice, and nothing here is a recommendation
          to buy or sell anything.
        </p>
        <p className="mt-4">
          <Link
            href="/about"
            className="underline hover:text-[var(--color-dim)]"
          >
            What this is, who runs it, and what it will never say
          </Link>
        </p>
        {/* The pages a stranger looks for before deciding whether to believe
            any of the above. Derived from ROUTES rather than listed again, so
            a page marked `inNav: false` cannot end up reachable only by
            somebody typing its address. */}
        <nav className="mt-4 flex flex-wrap gap-x-4 gap-y-2">
          {footerRoutes().map((r) => (
            <Link
              key={r.path}
              href={r.path}
              className="underline hover:text-[var(--color-dim)]"
            >
              {r.short ?? r.label}
            </Link>
          ))}
        </nav>
        {handle !== null && (
          <p className="mt-4 text-xs">
            <a
              href={handleHref(handle) ?? "#"}
              target="_blank"
              rel="noopener noreferrer"
              className="underline hover:text-[var(--color-dim)]"
            >
              @{handle} on X
            </a>
          </p>
        )}
      </div>
    </footer>
  );
}

export function App() {
  return (
    <div className="grid-ground flex min-h-screen flex-col">
      <Header />
      <main className="flex-1">
        <Switch>
          <Route path="/" component={Home} />
          {/* The live contest is retired (ADR 0038). What is left is the
              historical record of the weeks that ran while it did. */}
          <Route path="/payouts" component={History} />
          <Route path="/how-it-works" component={HowItWorks} />
          <Route path="/tokenomics" component={Token} />
          <Route path="/about" component={About} />
          {/* Not in ROUTES on purpose: a check is reached from the paste box or a
              shared link, never from the header, and ROUTES is the header and
              the footer. routes.test.tsx holds that table to exactly those. */}
          <Route path="/check/:address" component={Check} />
          <Route path="/privacy" component={Privacy} />
          <Route path="/terms" component={Terms} />
          <Route path="/contact" component={Contact} />
          {MOVED.map((m) => (
            <Route key={m.from} path={m.from}>
              <Redirect to={m.to} replace />
            </Route>
          ))}
          <Route>
            {/* Static hosting serves index.html for any path, so an unknown one
                reaches the router rather than the host. Said plainly: a blank
                page here would look like the site was broken. */}
            <div className="mx-auto max-w-5xl px-6 py-24">
              <h1 className="text-2xl font-semibold">No such page</h1>
              <p className="mt-3 text-[var(--color-dim)]">
                <Link href="/" className="underline">
                  Back to the start
                </Link>
              </p>
            </div>
          </Route>
        </Switch>
      </main>
      <Footer />
    </div>
  );
}
