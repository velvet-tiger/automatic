# Deploy to Acme

Run the Acme deployment pipeline for the current project.

## Steps

1. Verify the working tree is clean: `git status --porcelain`
2. Run the test suite: `npm run test:ci`
3. Build the project: `npm run build`
4. Determine the target environment:
   - If on `main` branch: deploy to **staging**
   - If a tagged commit: deploy to **production**
   - Otherwise: abort with a message
5. Run the deploy: `acme deploy --env {environment}`
6. Verify the deployment: `acme status --env {environment}`
7. Report the result including the deployment URL

## Important

- Never deploy to production without a tagged release.
- If tests fail, stop immediately and report the failures.
- If the deploy fails, run `acme deploy --rollback --env {environment}` and report.
