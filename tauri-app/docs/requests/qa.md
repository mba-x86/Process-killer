# QA agent: changes to SHARED files (all small and surgical)

1. ui/js/main.js - `ctx.every` / `ctx.onCleanup` are now bound to the navigation that received the ctx (per-page ctx via
   Object.create(ctx) + `reg(owner, fn)`). Before, any `every()/onCleanup()` called after an `await` in a page (dashboard,
   activity, storage, optimizer ...) was registered into the NEXT page's cleanup list after a fast navigation, so the abandoned
   page's intervals/listeners kept running. Late registrations from a left page are now stopped/run immediately.
2. ui/js/ui.js `modal()` - focus moves into the dialog (primary/danger button, else the dialog) on open and returns to the
   previously focused element on close. Without it, Tab went to controls behind the overlay (keyboard a11y).
