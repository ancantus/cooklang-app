# Cooklang Android Client — Product Plan

This document records product and technical decisions for the first useful version of the app. Update the decision log when a choice changes so the reasoning stays visible.

## Product direction

Build an Android app with Dioxus that reads a user's Cooklang recipe files from a GitHub repository and presents them as a browsable, readable recipe collection. Later, the app may let the user edit recipes and commit those changes back to GitHub.

## Decisions made

| ID | Decision | Reason / effect |
| --- | --- | --- |
| D-1 | Keep the current Android app as the product target. | The repository is already configured as an Android app, with Android setup documented in `SETUP.md`. |
| D-2 | Keep Rust and Dioxus 0.7 as the app stack. | This is the existing project stack; no framework migration is in scope. |
| D-3 | Use a GitHub repository as the source of the recipe collection. | The app should render Cooklang files stored in that repository. |
| D-4 | Make the first milestone read-only. | Establish repository connection, recipe discovery, and rendering before taking on write access and commit behavior. |
| D-5 | Treat editing and committing as a later milestone. | They are part of the longer-term direction, but not required to validate the read experience. |
| D-6 | Keep the first release focused on recipes. | Shopping lists, meal plans, and other Cooklang ecosystem features are not part of the initial scope. |
| D-7 | Support public and private GitHub repositories in v1. | Private recipes are a plausible core use case; this means v1 needs authenticated access. |
| D-8 | Connect one selected repository in v1. | Keeps initial setup and navigation centered on a single recipe collection. |
| D-9 | Search the selected repository, including nested folders, for Cooklang recipes. | Users should not need to move files into a required `recipes/` directory. |
| D-10 | Refresh repository content on demand in v1. | Load updates when the app opens or the user refreshes; defer background sync. |
| D-11 | Use a structured, read-only recipe detail view in v1. | Show available metadata, ingredients with quantities and optional labels, ordered method steps with section headings and timers, and notes separately. Preserve unsupported syntax as readable text. Defer checklists, serving scaling, active timer controls, images, and raw-source editing. |
| D-12 | Discover only files ending in `.cook`. | This is the documented Cooklang recipe extension; other extensions are out of scope unless later evidence requires them. |
| D-13 | Offer both manual repository entry and a GitHub repository picker. | Manual entry supports arbitrary public repositories; the picker lists repositories accessible through an installation of the app and lets the user authorize another. Only one repository is connected at once. |
| D-14 | Sort recipes alphabetically by title and show the repository path when titles collide. | Gives predictable browsing order while keeping duplicate titles distinguishable. |
| D-15 | Provide search by recipe title and tags. | This is part of the recipe list experience. |
| D-16 | Use a GitHub App with browser authorization code + PKCE and a backend token exchange to obtain a user access token. | Keep the app credential off-device. Request read-only access in v1; this token can authenticate HTTPS Git while acting on the user's behalf, leaving a path to user-authorized writes later. |
| D-17 | Limit v1 search to recipe titles and tags, case-insensitively as the user types. | Meets the requested search behavior without indexing recipe instructions or notes. |
| D-18 | Use the local Git checkout as the recipe cache. | Keep the last successfully fetched checkout in app-private storage for quick/offline reading, and delete it when its repository is disconnected. |
| D-19 | Use HTTPS Git operations for repository content sync. | Clone/fetch/pull into an app-local checkout and read recipe files through Rust filesystem APIs. Use Git for later branch/commit/push work too. Keep GitHub REST calls for tasks Git cannot do, such as GitHub App installation/token management and listing authorized repositories. |
| D-20 | Use the GitHub App user access token, rather than an installation token, as the Git HTTPS credential. | This keeps Git operations user-authorized for the eventual edit/commit flow. The GitHub App still limits repository access and permissions. |
| D-21 | Use GitHub's App installations API for the repository picker and authorization flow. | List repositories the signed-in user can access through app installations and let them authorize access to another repository. Keep recipe transfer on Git. |
| D-22 | Use the repository's default branch only in v1. | Keeps the initial connection and refresh behavior predictable; defer branch switching. |
| D-23 | Keep showing the last local checkout when refresh fails. | Clearly mark it as stale/offline and offer retry, preserving recipe access when GitHub is unavailable. |
| D-24 | Show title, tags, and available total time in recipe list rows. | Keep full metadata in the detail view and show paths where duplicate titles need disambiguation. |
| D-25 | Treat the local checkout as the source of truth while offline; sync it from the upstream default branch when online. | On successful sync, show the updated files. If sync fails, keep using the last local files and show stale status. The v1 checkout is read-only, so it has no local edits to merge. |
| D-26 | Use the official `cooklang-rs` parser crate for Cooklang files. | It is the canonical Rust parser and provides the structure needed for ingredients, steps, metadata, cookware, and timers. Validate against representative files; keep optional extensions disabled unless needed. |
| D-27 | Use a shallow Git clone in v1. | Reduces initial storage and network transfer. Revisit if later editing or branch workflows need more history. |

## First release scope

### Included

- Connect to one public or private GitHub repository.
- Find Cooklang recipe files across the repository, including nested folders.
- Show a recipe list with useful identifying information.
- Search the recipe list by title and tags.
- Open a recipe and render its content in a readable format, including ingredients and method where the available parser supports them.
- Show useful loading, empty, and error states, including when a repository has no recipes or cannot be reached.
- Refresh the default branch in the local Git checkout on app open or on user request; retain the last fetched checkout for offline reading and clear it on disconnect.

### Deferred

- Editing recipes in the app.
- Creating or deleting recipe files.
- Committing or pushing changes to GitHub.
- Multiple repositories, recipe discovery across GitHub, and sharing workflows.
- Background synchronization beyond what is needed for a basic refreshable read flow.
- Non-recipe Cooklang files and broader cooking tools.

## Initial user flow

1. The user supplies or selects a GitHub repository.
2. The app obtains access to the repository using the GitHub App user authorization flow.
3. The app clones or fetches the repository into an app-private local checkout, then reads `.cook` files and displays a recipe list.
4. The user opens a recipe and reads its rendered content.
5. The user can refresh to load the latest repository version.

## Working technical shape

- Keep the UI in the existing Dioxus app.
- Keep Git operations, local repository file access, and Cooklang parsing behind Rust modules/services so UI components do not own synchronization or parsing details.
- Use a Rust Git library embedded in the app; Android does not provide a system `git` executable. Compare `gix` (pure Rust, HTTPS transport support) with `git2` (Rust bindings to C libgit2). Verify clone/fetch/authentication on Android before choosing. The `gix` project's roadmap still identifies some core workflows as stabilization work, so its reliability for this app needs a compatibility spike. See the [gitoxide project status](https://github.com/GitoxideLabs/gitoxide) and [`git2` documentation](https://docs.rs/git2/latest/git2/).
- Use the official `cooklang-rs` parser crate, with parser extensions disabled unless needed. It is the canonical Rust parser and exposes metadata, ingredients, steps, cookware, and timers. Validate parser output against representative files. See [Cooklang's developer docs](https://cooklang.org/docs/for-developers/) and the [parser repository](https://github.com/cooklang/cooklang-rs).
- Model loading, loaded, empty, and failed states explicitly in the UI.
- Avoid coupling rendering to one repository directory layout until the repository discovery rule is decided.
- Investigate an existing Rust Cooklang parser before deciding whether to use a dependency or implement a limited parser. Do not treat raw Cooklang syntax as structured data without validating the parser's coverage.

These are implementation boundaries, not decisions about specific crates, APIs, or data models.

## Recipe detail decision (Q-4)

Cooklang carries more structure than plain ingredient mentions: files can include YAML metadata, paragraphs as steps, named sections, ingredient quantities and notes, cookware, timers, optional ingredients, and separate notes. The proposal is to render that structure into a practical recipe reader:

1. **Header:** title from metadata, falling back to the filename; show available serving count, prep/cook/total time, tags, and description as compact metadata.
2. **Ingredients:** show a clean ingredient list with quantities and units, clearly mark optional ingredients, and retain preparation notes such as “finely chopped.” Keep recipe references recognizable as linked/sub-recipe ingredients when their target can be resolved.
3. **Method:** show authored steps in order, preserving named section headings. Render ingredient and cookware markers as readable words, and show timers inline with the step that uses them.
4. **Other authored content:** show Cooklang notes separately from method steps. If a construct is unsupported or malformed, preserve its readable source text instead of silently dropping it.

This would be a read-only view: no ingredient checklist, serving scaler, timer controls, image handling, or editable raw-source mode in the first pass. Those could be added later if desired. The official [Cooklang specification](https://cooklang.org/docs/spec/) defines the recipe syntax; the [conventions](https://cooklang.org/docs/conventions/) describe common metadata such as servings, times, tags, and images. The spec cautions that applications do not necessarily support every latest feature, so exact parser coverage needs validation against real collection files.

**Decision:** use this structured recipe reader as the v1 detail view. It makes the Cooklang annotations useful while keeping the first release focused on reading. Preserve unrecognized content rather than hide it.

## Implementation validation before build

- Compare `gix` and `git2` with an Android build spike. Confirm clone/fetch over HTTPS and credential injection work on Android before choosing a library.
- Validate `cooklang-rs` output against representative recipes, especially metadata, sections, optional ingredients, notes, timers, and unsupported syntax fallback.
- Confirm the shallow-clone refresh strategy and document how editing/pushing will deepen history if that becomes necessary.
- Confirm GitHub App user-token exchange, repository picker permissions, and token storage requirements before implementing private-repository access.

No product decisions remain open from this planning session. These validation items need evidence before implementation choices are locked in.

## Suggested milestones

1. **Confirm product choices:** answer the open questions needed to define repository selection, access, file discovery, and rendered recipe content.
2. **Repository read spike:** prove the chosen GitHub App user-token flow and HTTPS Git clone/fetch on Android, then read a small set of Cooklang files from the local checkout.
3. **Recipe browser:** replace the placeholder screen with repository setup, recipe list, recipe detail, refresh, and failure/empty states.
4. **Read experience hardening:** validate against representative Cooklang files and decide whether basic local caching is needed.
5. **Editing and commits:** define an edit flow, conflict handling, commit message behavior, and write permissions after the read-only flow is working.

## First release acceptance criteria

- A user can connect the app to the chosen kind of GitHub repository on Android.
- A user can enter a public repository or select a repository accessible through an app installation; they can authorize the app for another repository.
- The app discovers supported Cooklang files according to the agreed path and extension rules.
- A user can browse discovered recipes and open one.
- A user can search by title and tags and read the last fetched collection offline; disconnecting clears the local checkout.
- The detail view presents recipe content in the agreed readable structure.
- Empty repositories, network/API errors, and refresh behavior are understandable to the user.
- No recipe changes are written back to GitHub in this milestone.

## Change log

- 2026-11-15: Initial plan written from the current project state and product direction.
- 2026-11-15: Recorded support for private and public repositories, one selected repository, recursive recipe discovery, and on-demand refresh. Added a proposed structured recipe detail view for user decision.
- 2026-11-15: Decided on the structured recipe detail view, `.cook`-only discovery, both repository entry and picker, alphabetical ordering with path disambiguation, and title/tag search. Authentication, search matching, caching, and API access remained open.
- 2026-11-15: Chose a GitHub App with browser authorization code + PKCE and a backend token exchange, and title/tag-only case-insensitive live search. Cache behavior and API retrieval approach remained open.
- 2026-11-15: Replaced REST tree/content retrieval with a local Git checkout used as both sync mechanism and cache, with Rust filesystem reads for recipes. Kept the GitHub App user access token as the Git credential to align with future user-authorized edits and commits. Decided the picker lists repositories available through app installations and lets users authorize more; GitHub's App installations API supplies the list.
- 2026-11-15: Decided to use the repository default branch, keep cached recipes visible with stale status after failed refresh, and show title/tags/available total time in list rows. Chose the official `cooklang-rs` parser and defined Git sync as updating a read-only local checkout from upstream while preserving cached files on failure. Identified `gix`/`git2` as Android Git-library candidates requiring a compatibility spike.
- 2026-11-15: Chose a shallow clone for v1 to reduce storage and network use; revisit before editing or branch workflows.
