## Summary

<!-- What changed and why. -->

## Checklist

- [ ] `cargo fmt --all --check` and `cargo clippy --all-targets -- -D warnings` pass
- [ ] `cargo test` passes
- [ ] Desktop: `npm run typecheck && npm run build` pass (if `apps/desktop` changed)
- [ ] New cleanup rules have a risk level, a description and are platform-gated
- [ ] Nothing touches protected locations or deletes without confirmation
