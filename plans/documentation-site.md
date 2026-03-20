# Plan: Redtrail Documentation Site

> Source PRD: docs/prd-documentation-site.md

## Architectural decisions

Durable decisions that apply across all phases:

- **Framework**: Astro Starlight, site root at `docs-site/`
- **Theming**: Catppuccin Mocha (dark default) + Latte (light). Red primary accent, green secondary accent. CSS custom properties mapped from Catppuccin palette.
- **i18n**: Starlight native i18n, directory-based (`src/content/docs/en/`, `src/content/docs/es/`). English is default locale. Content written in English first, then translated.
- **Custom components**: MDX components live in `docs-site/src/components/`. Terminal Frame and Hero are the two launch components.
- **Testing**: Vitest for component unit tests. Test files colocated with components or in `docs-site/tests/`.
- **Labs**: Docker Compose environments live in `docs-site/labs/simple/` and `docs-site/labs/complex/`. Each lab has its own `docker-compose.yml` and `README.md`.
- **Hosting**: GitHub Pages deployed via `.github/workflows/docs.yml`. Triggers on push to main, builds Starlight, deploys artifact.
- **Search**: Pagefind (Starlight built-in), zero config.
- **Logo**: Text-only placeholder. No image asset required at launch.
- **Content structure**: Landing → Getting Started → Core Concepts → Guides → CLI Reference → Skills → Configuration → Contributing

---

## Phase 1: Themed Starlight skeleton + GitHub Pages deploy

**User stories**: 13, 14

### What to build

Initialize an Astro Starlight project in `docs-site/`. Configure the Catppuccin Mocha/Latte color scheme via Starlight's CSS custom property overrides, with red as the primary accent and green as the secondary. Set up the dark/light mode toggle. Configure Starlight's sidebar with the full content structure (all sections, even though most will be empty). Add the i18n configuration for English and Spanish with a placeholder index page in each locale. Create the GitHub Actions workflow that builds the Starlight site and deploys to GitHub Pages on push to main. The end result is a live, themed, empty docs site accessible at the GitHub Pages URL.

### Acceptance criteria

- [ ] `npm run build` succeeds in `docs-site/`
- [ ] Site renders with Catppuccin Mocha colors in dark mode and Latte in light mode
- [ ] Red accent color visible on links/highlights, green on success-styled elements
- [ ] Dark/light toggle works and persists preference
- [ ] Sidebar shows all 8 content sections
- [ ] `/en/` and `/es/` routes both resolve to placeholder content
- [ ] GitHub Actions workflow builds and deploys successfully on push to main
- [ ] Pagefind search input is present and functional (even if no content to search yet)

---

## Phase 2: Terminal Frame component + Vitest test suite

**User stories**: 10

### What to build

Set up Vitest in the `docs-site/` project with Astro component testing support. Build the Terminal Frame MDX component — a styled wrapper that renders code examples inside a terminal emulator UI. The component accepts props: title (window title bar text), prompt (customizable prompt string), and content (the command + output). It must be theme-aware, rendering with Mocha colors in dark mode and Latte in light mode. The terminal chrome (title bar with traffic light dots, rounded corners, subtle shadow) should feel like a real terminal window. Write Vitest unit tests covering: rendering with all prop combinations, theme-aware class application, edge cases (empty content, very long output, multiline commands). Add a demo page in the docs that showcases the component so it can be visually verified.

### Acceptance criteria

- [ ] Vitest runs via `npm test` in `docs-site/`
- [ ] Terminal Frame component renders a styled terminal window with title bar and traffic light dots
- [ ] Component accepts and correctly renders `title`, `prompt`, and `content` props
- [ ] Component applies Mocha styles in dark mode and Latte styles in light mode
- [ ] Unit tests pass for all prop combinations and edge cases
- [ ] Demo page exists showing the component with various configurations

---

## Phase 3: Hero landing page with tiered audience paths

**User stories**: 14, 15

### What to build

Replace the default Starlight index page with a custom Hero component. The hero section features the "redtrail" text logo, a one-line value proposition ("Pentesting orchestrator that thinks with you"), and a visual terminal demo (using the Terminal Frame component from Phase 2) showing a brief RT session — a few commands with realistic output that convey the tool's feel in seconds. Below the hero, three audience path cards: "Quick Start" (experienced pentesters, links to Getting Started), "Understand the Methodology" (intermediate, links to Core Concepts), and "Hands-On Tutorial" (beginners, links to Guides). Each card has a short description and icon/visual. The hero must work in both English and Spanish, pulling localized strings. Write Vitest tests for the hero component: tiered navigation renders correctly, links point to correct locale-prefixed routes, responsive layout works.

### Acceptance criteria

- [ ] Custom hero replaces Starlight default on the index page
- [ ] Text logo "redtrail" is displayed prominently
- [ ] Terminal Frame demo shows a realistic RT session snippet
- [ ] Three audience path cards render with correct labels and links
- [ ] Hero content is localized — English and Spanish versions render correctly
- [ ] Links on cards point to correct locale-prefixed routes (`/en/getting-started/`, `/es/getting-started/`, etc.)
- [ ] Vitest tests pass for hero rendering, localization, and link targets
- [ ] Responsive layout works on mobile and desktop viewports

---

## Phase 4: Getting Started content (en + es)

**User stories**: 1, 11

### What to build

Write the full Getting Started guide in English, then translate to Spanish. This is the fast path for experienced pentesters — it should be completable in under 10 minutes of reading. Content flow: install RT (cargo install or binary), run `rt setup` (first-run wizard, LLM provider config), create a workspace with `rt init`, run a first recon command and see it populate the KB with `rt kb`, form a hypothesis with `rt hypothesis`, ask the advisor with `rt ask`, generate a report with `rt report`. Every command example uses the Terminal Frame component to show exact input and expected output. The guide should assume the reader knows pentesting but not RT — no hand-holding on what nmap does, full explanation of what RT does with the output. End with a "Next steps" section pointing to Core Concepts (for methodology) and Tutorials (for hands-on practice).

### Acceptance criteria

- [ ] Getting Started page exists at `/en/getting-started/` and `/es/getting-started/`
- [ ] Covers full flow: install → setup → init → recon → KB → hypothesis → ask → report
- [ ] Every RT command shown with Terminal Frame component
- [ ] English content is technically accurate against current RT CLI behavior
- [ ] Spanish translation covers the same content completely
- [ ] "Next steps" section links to Core Concepts and Tutorials
- [ ] Page renders correctly in both dark and light mode
- [ ] Completable in under 10 minutes of reading

---

## Phase 5: Simple lab Docker environment

**User stories**: 4, 6, 17

### What to build

Create the simple lab environment in `docs-site/labs/simple/`. A single Docker container running: an HTTP web application (with a login page using default admin credentials and an admin panel with command injection), an FTP service with anonymous login enabled (containing decoy files that look interesting but lead nowhere — the red herring), and SSH. The web app should be a minimal custom application (Flask or Express) purpose-built for this lab — not an existing vulnerable app. The FTP decoy files should be plausible enough to investigate (e.g. `backup.sql.gz` that contains junk, `credentials.txt` with expired/fake creds). A flag file readable only via the command injection → shell path proves completion. Write a `README.md` with setup instructions (`docker compose up`) and teardown. Write Docker smoke tests that verify: container boots, HTTP returns 200, FTP allows anonymous login, SSH accepts connections. The attack path must be solvable end-to-end with RT commands.

### Acceptance criteria

- [ ] `docker compose up` in `docs-site/labs/simple/` boots the environment successfully
- [ ] HTTP service responds on port 80 with a web application
- [ ] FTP service allows anonymous login and lists decoy files
- [ ] SSH service accepts connections
- [ ] Default admin credentials grant access to the admin panel
- [ ] Admin panel has a command injection vulnerability that yields a shell
- [ ] Flag file is readable only through the exploitation path
- [ ] FTP decoy files are plausible but clearly a dead end upon investigation
- [ ] `docker compose down` cleanly tears down the environment
- [ ] Smoke tests pass in CI
- [ ] README documents setup, teardown, and resource requirements

---

## Phase 6: Simple lab tutorial content (en + es)

**User stories**: 3, 6, 11, 17

### What to build

Write the full beginner tutorial in English, then translate to Spanish. This is a step-by-step walkthrough of solving the simple lab using RT. The tutorial starts with spinning up the lab (`docker compose up`), then walks through: initializing a workspace (`rt init`), running nmap recon and ingesting results, discovering the three services, investigating the FTP red herring (form hypothesis, gather evidence, refute it), discovering default web credentials, accessing the admin panel, finding and exploiting the command injection, capturing the flag, and generating a report. Every step shows the exact RT command with Terminal Frame component output. The red herring section is critical — it must clearly demonstrate the hypothesis lifecycle: create hypothesis ("FTP files may contain credentials") → probe (inspect files) → add evidence (files are junk) → refute hypothesis. The tutorial should explain the *why* at each step — why RT suggests this, why we form this hypothesis, why this evidence refutes it. Target completion time: ~30 minutes hands-on.

### Acceptance criteria

- [ ] Tutorial page exists at `/en/guides/simple-lab/` and `/es/guides/simple-lab/`
- [ ] Covers the complete lab from `docker compose up` to `rt report`
- [ ] Every RT command shown with Terminal Frame component and expected output
- [ ] FTP red herring section clearly demonstrates hypothesis creation → probing → refutation
- [ ] Tutorial explains the reasoning behind each step, not just the commands
- [ ] Spanish translation covers the same content completely
- [ ] A beginner following step-by-step can complete the lab in ~30 minutes
- [ ] Tutorial references match the actual lab environment behavior from Phase 5

---

## Phase 7: Stub pages for remaining sections (en + es)

**User stories**: 2, 12, 16

### What to build

Create placeholder pages for all remaining content sections in both English and Spanish. Each stub should have a title, a one-paragraph description of what the section will cover, and a "Coming soon" notice. Sections: Core Concepts (workspace model, knowledge base, hypotheses/evidence, deductive layers L0-L4, BISCL framework), CLI Reference (list all `rt` subcommands with one-line descriptions, even if full docs aren't written yet), Skills (skill system overview, built-in skill list, custom skill development), Configuration (global config, workspace config, LLM provider setup), Contributing (architecture overview, how to extend RT). The CLI Reference stub should at minimum list every subcommand with its `--help` one-liner so it has immediate utility even as a stub. Ensure all stubs appear correctly in the sidebar navigation.

### Acceptance criteria

- [ ] Core Concepts stub exists in both locales with section description
- [ ] CLI Reference stub lists all `rt` subcommands with one-line descriptions
- [ ] Skills stub exists in both locales with section description
- [ ] Configuration stub exists in both locales with section description
- [ ] Contributing stub exists in both locales with section description
- [ ] All stubs render correctly in sidebar navigation
- [ ] Each stub has a clear "Coming soon" indicator
- [ ] Pagefind indexes the stub content (subcommand names are searchable)

---

## Phase 8: Complex lab Docker environment

**User stories**: 5, 7, 8, 9, 18, 19, 20

### What to build

Create the complex lab environment in `docs-site/labs/complex/`. Four Docker containers on a custom bridge network simulating a corporate topology:

**web01 (DMZ, deep target):** HTTP web application + SSH. The web app is secured but has a hidden admin panel discoverable via directory enumeration. The main application has a stored XSS vulnerability that can exfiltrate the admin's session cookie (simulate with a bot/script that periodically visits pages with a valid admin session). The admin panel (accessible with the stolen cookie) has an RCE vulnerability in a file upload or template rendering feature. Red herrings: an outdated Apache/nginx version header suggesting a known CVE that doesn't actually apply, a `/api/debug` endpoint that looks promising but returns nothing useful, potentially a robots.txt hinting at paths that don't exist.

**app01 (internal):** HTTP application + SSH. Credentials reused from web01 grant SSH access. The application has a command injection vulnerability in a search or export feature. A config file on disk contains database credentials for db01.

**db01 (internal):** MySQL service. Weak credentials found in app01's config grant access. The database contains a table with user dumps including dc01 admin credentials. A file-read privilege escalation path exists (MySQL `LOAD_FILE` or similar).

**dc01 (final target):** SSH + a custom service. Credentials from the db01 dump grant admin access. A flag file proves full compromise.

Network configuration: web01 is reachable from the host. app01, db01, dc01 are on an internal-only network. web01 bridges both networks (DMZ pivot point). Write a `docker-compose.yml` with health checks and service dependencies. Write smoke tests verifying all containers boot and expected services respond. Write a README documenting the topology, setup, teardown, and resource requirements.

### Acceptance criteria

- [ ] `docker compose up` boots all 4 containers and both networks
- [ ] web01 is reachable from the host; app01/db01/dc01 are not directly reachable
- [ ] web01: HTTP responds, admin panel is discoverable via enumeration, XSS works, cookie exfiltration leads to admin access, RCE works in admin panel
- [ ] web01: red herrings are present (fake CVE version, debug endpoint, misleading robots.txt)
- [ ] app01: SSH accepts web01 credentials, command injection works, config file contains db01 credentials
- [ ] db01: MySQL accepts credentials from app01 config, user dump table contains dc01 credentials, file-read escalation works
- [ ] dc01: credentials from db01 dump grant admin access, flag file is readable
- [ ] Full chain is solvable: web01 → app01 → db01 → dc01 → flag
- [ ] Health checks ensure services are ready before dependent containers start
- [ ] `docker compose down` cleanly tears down all containers and networks
- [ ] Smoke tests pass in CI
- [ ] README documents topology diagram, setup, teardown, and resource requirements (~2-4GB RAM)
