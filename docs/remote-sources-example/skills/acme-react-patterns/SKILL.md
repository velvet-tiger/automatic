# Acme React Patterns

You are working in an Acme project that uses React 19 with TypeScript.

## Component Conventions

- Use functional components with explicit prop interfaces.
- Name components in PascalCase. File name must match: `UserProfile.tsx`.
- Co-locate styles, tests, and stories in the same directory.

## Hooks

- Prefix custom hooks with `use`: `useAcmeAuth`, `useProductSearch`.
- All hooks must have explicit return types.
- Avoid `useEffect` for data fetching; use Acme's `useQuery` wrapper instead.

## State Management

- Local UI state: `useState`.
- Server state: `useQuery` / `useMutation` (wraps TanStack Query).
- Global app state: Acme's `useAppStore` (Zustand-based).
- Never use Redux or Context for server-state caching.

## File Structure

```
src/components/
  UserProfile/
    UserProfile.tsx        # Component
    UserProfile.test.tsx   # Tests
    UserProfile.stories.tsx # Storybook
    index.ts               # Re-export
```

## Error Boundaries

Every route-level component must be wrapped in `<AcmeErrorBoundary>`. Use the `fallback` prop for custom error UI.

## Imports

```typescript
// External libraries first
import { useState } from 'react';
import { useQuery } from '@acme/data';

// Internal modules
import { Button } from '@acme/ui';
import { formatCurrency } from '@acme/utils';

// Relative imports last
import { UserAvatar } from './UserAvatar';
```
