# PRD: Redtrail Documentation Site

## Problem Statement

Redtrail has no user-facing documentation. The tool has a rich feature set (CLI commands, skill system, deductive methodology, knowledge base, hypothesis tracking) but no way for new users to learn it. Without clear, educative, and visually appealing documentation, adoption is blocked — pentesters won't invest time in a tool they can't quickly understand.

The documentation must serve three audiences simultaneously: experienced pentesters who want to see the workflow fast, intermediate security practitioners who want to understand the methodology, and beginners who need full hand-held walkthroughs. It must go beyond static text — users need to be able to run real vulnerable environments and follow along with RT commands.

## Solution

Build a documentation site using Astro Starlight inside the redtrail repo (`docs-site/`), themed with Catppuccin (Mocha dark / Latte light) and red/green accent colors. The site ships with two Docker Compose lab environments (simple and complex) that users can spin up locally and solve using RT.

The launch scope is narrow but deep: a polished Getting Started guide and one complete tutorial against the simple lab. Other sections (Core Concepts, CLI Reference, Skills, Configuration, Contributing) exist as stubs. The site is bilingual (English and Spanish), hosted on GitHub Pages, and deployed via GitHub Actions.

Custom MDX components (terminal frame, hero page) give the site a distinctive visual identity that sets it apart from generic documentation.

## User Stories

1. As an experienced pentester, I want a quick start guide that shows me the RT workflow in under 10 minutes, so that I can decide if the tool is worth adopting.

2. As an intermediate security practitioner, I want an overview of RT's deductive layering system (L0-L4) and BISCL framework, so that I can understand the methodology behind the tool.

3. As a beginner, I want a step-by-step tutorial I can follow along with on a real vulnerable environment, so that I can learn both pentesting methodology and RT simultaneously.

4. As a user, I want to spin up a simple lab environment with `docker compose up`, so that I can practice RT commands against a real target.

5. As a user, I want to spin up a complex corporate lab environment with multiple hosts, so that I can experience RT's multi-host KB, credential pivoting, and attack graph features.

6. As a user following the simple lab tutorial, I want to encounter at least one red herring (e.g. FTP anonymous access leading nowhere), so that I can learn how RT tracks and refutes hypotheses.

7. As a user following the complex lab, I want web01 to be a deep multi-step challenge (enumeration → admin panel discovery → XSS → cookie exfiltration → admin RCE), so that I can experience the full depth of hypothesis-driven testing.

8. As a user following the complex lab, I want to find credentials on one host that grant access to another, so that I can see how RT's KB cross-references findings across hosts.

9. As a user, I want the complex lab to have red herrings and dead-end paths on each host, so that I can experience the realistic process of forming hypotheses, testing them, and refuting the wrong ones.

10. As a user, I want code examples displayed in a terminal-like UI component, so that I can clearly see what commands to run and what output to expect.

11. As a Spanish-speaking user, I want the full documentation available in Spanish, so that I can learn in my native language.

12. As a user, I want to search the documentation, so that I can quickly find information about specific commands or concepts.

13. As a user, I want the site to have both dark and light mode, so that I can read comfortably in any environment.

14. As a user, I want a visually appealing landing page that communicates what RT does, so that I immediately understand the tool's value proposition.

15. As a user landing on the hero page, I want to see tiered paths (quick start / methodology / full tutorial), so that I can self-select the depth that matches my experience level.

16. As a contributor, I want a Contributing stub page, so that I know where to look when the project is ready for contributions.

17. As a user, I want the simple lab to be completable in ~30 minutes, so that I don't need a huge time investment to experience RT's value.

18. As a user, I want the complex lab's later hosts (app01, db01, dc01) to be lighter than web01, so that the lab demonstrates pivoting and KB value without repeating the same depth of false leads.

19. As a user exploring the complex lab's web01, I want to discover an admin panel through enumeration rather than it being obvious, so that the recon phase feels authentic.

20. As a user, I want the complex lab to have a corporate topology (DMZ web server → internal app server → database → domain controller), so that it resembles a real engagement.

## Implementation Decisions

### Framework & Tooling
- **Astro Starlight** as the SSG — MDX support, built-in i18n, Expressive Code for syntax highlighting, Pagefind search out of the box
- **Vitest** for frontend component testing
- Site lives in `docs-site/` directory inside the redtrail repo

### Theming
- **Catppuccin Mocha** (dark mode default) and **Catppuccin Latte** (light mode)
- **Red** as primary accent color, **green** as secondary accent (success states)
- Text-only logo placeholder — real logo to be added later

### Custom Components
- **Terminal Frame component**: reusable MDX component that renders command examples inside a styled terminal emulator UI. Accepts props (title, prompt style, output). Theme-aware (Mocha/Latte).
- **Hero page component**: custom landing page with value proposition, visual terminal demo, and tiered audience navigation paths.

### Content Structure
1. **Landing/Hero** — what is RT, value prop, tiered audience paths
2. **Getting Started** — install, `rt setup`, `rt init`, first session (launch scope: fully written)
3. **Core Concepts** — workspace model, KB, hypotheses, deductive layers, BISCL (launch scope: stub)
4. **Guides/Tutorials** — simple lab walkthrough (launch scope: fully written), complex lab (launch scope: stub)
5. **CLI Reference** — all `rt` subcommands (launch scope: stub)
6. **Skills** — skill system, built-in catalog, custom skills (launch scope: stub)
7. **Configuration** — global config, workspace config, LLM providers (launch scope: stub)
8. **Contributing** — architecture for contributors (launch scope: stub)

### Internationalization
- English and Spanish from day one
- Starlight's native i18n system (directory-based: `en/`, `es/`)

### Simple Lab (`docs-site/labs/simple/`)
- **Single host** with SSH, HTTP, FTP
- **Attack path**: nmap recon → FTP anon login (red herring, decoy files) → web app with default admin credentials → admin panel with command injection → shell → flag
- **~30 minutes** to complete following the tutorial
- Docker Compose with a single container

### Complex Lab (`docs-site/labs/complex/`)
- **4 containers** on a custom Docker network simulating a corporate environment
- **web01 (DMZ, deep)**: enumeration → discover hidden admin panel → main app has XSS → exfiltrate admin cookie → access admin panel → RCE in admin functionality. 2-3 red herrings (e.g. outdated service version suggesting a CVE that doesn't apply, suspicious but useless endpoint).
- **app01 (internal, lighter)**: credential reuse from web01 grants access, application has command injection
- **db01 (internal, lighter)**: weak DB credentials found in app01 config files, DB has file read privilege escalation
- **dc01 (final target, lightest)**: credentials from DB dump grant admin access
- Each host has plausible dead-end paths to exercise hypothesis refutation

### Hosting & Deployment
- **GitHub Pages** via GitHub Actions workflow
- Custom domain deferred to later
- Workflow triggers on push to main, builds Starlight, deploys to Pages

### Search
- **Pagefind** (Starlight built-in) — client-side, zero external dependencies

## Testing Decisions

Good tests verify external behavior from the user's perspective, not implementation details. A test should break only when the feature it tests is actually broken, not when internal code is refactored.

### Frontend Component Tests (Vitest)
- **Terminal Frame component**: renders correctly with various prop combinations, respects theme (Mocha/Latte classes), handles edge cases (empty output, long content)
- **Hero page component**: renders tiered navigation paths, theme toggle works, responsive layout
- **i18n rendering**: components render correctly in both English and Spanish contexts

### Lab Environment Tests
- **Docker smoke tests**: containers boot successfully, expected services respond on expected ports (HTTP 200 on web ports, SSH banner on 22, etc.)
- **Service validation**: vulnerable services are actually exploitable (the vulns work as designed)
- These run in CI to catch regressions if Dockerfiles are modified

### Site Build Test
- The GitHub Actions workflow itself serves as the build test — if Starlight fails to build, deploy fails
- Can also run `npm run build` locally / in CI as a pre-merge check

## Out of Scope

- Custom logo or brand illustrations (text placeholder for now)
- Custom domain setup (deferred)
- Full content for stub sections (Core Concepts, CLI Reference, Skills, Configuration, Contributing)
- Complex lab tutorial content (the environment ships, the written walkthrough comes later)
- Video tutorials or screencasts
- Blog section
- Community/forum integration
- Analytics or telemetry on the docs site
- Versioned documentation (premature before v1.0)
- TUI documentation (TUI is being removed)

## Further Notes

- The complex lab's web01 is intentionally the deepest target — it's where the tutorial demonstrates the full hypothesis lifecycle (create → probe → refute → refine → confirm). Later hosts (app01, db01, dc01) are lighter because the user already understands the methodology by then; those hosts exist to showcase multi-host KB features and credential pivoting.
- The Catppuccin color palette is well-documented at https://catppuccin.com — exact hex values for Mocha and Latte flavors should be mapped to Starlight's CSS custom properties.
- Expressive Code (bundled with Starlight) already supports terminal-frame rendering for code blocks. The custom Terminal Frame component extends this with richer interactivity and RT-specific styling.
- Content should be written in English first, then translated to Spanish. Starlight's i18n system uses parallel directory structures (`src/content/docs/en/`, `src/content/docs/es/`).
- Launch priority is narrow-but-deep: ship a killer Getting Started + simple lab tutorial before filling in reference sections.
