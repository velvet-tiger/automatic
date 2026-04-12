# Acme Deployment

You are deploying an Acme service. Follow this procedure exactly.

## Environments

| Environment | Branch | URL |
|-------------|--------|-----|
| Development | `develop` | `dev.acme.internal` |
| Staging | `main` | `staging.acme.dev` |
| Production | tagged releases | `app.acme.dev` |

## Pre-Deploy Checklist

1. All tests pass: `npm run test:ci`
2. Lint clean: `npm run lint`
3. Build succeeds: `npm run build`
4. Database migrations are reversible

## Deploy Command

```bash
# Staging (automatic on merge to main)
acme deploy --env staging

# Production (requires tagged release)
acme deploy --env production --tag v2.1.0
```

## Rollback

```bash
acme deploy --env production --rollback
```

This reverts to the previous deployment. Always verify with `acme status --env production` after rollback.

## Secrets

Never commit secrets. All environment variables are managed via `acme secrets set KEY=VALUE --env staging`. The deploy process injects them at runtime.
