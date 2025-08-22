Pre-commit Hook
----------------

Enable the repo’s pre-commit hook to run checks before each commit:

```
git config core.hooksPath .githooks
```

This sets Git to use hooks from `.githooks/`. The `pre-commit` hook runs `./check.sh` and will block commits if checks fail.
