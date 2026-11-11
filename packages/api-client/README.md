# @batvision/api-client

TypeScript client **generated** from the server's OpenAPI description. Never edit `openapi.json` or `src/schema.d.ts` by hand.

After changing the server API:

```bash
pnpm --filter @batvision/api-client generate
```

CI fails if the generated files no longer match the server.
