# Gemini OAuth Discovery Notes

**Status**: Pending discovery

This document captures findings from analyzing Gemini CLI's OAuth implementation.

---

## Instructions

Before implementing, run these commands to discover the OAuth configuration:

```bash
# Clone Gemini CLI
git clone --depth 1 https://github.com/google-gemini/gemini-cli /tmp/gemini-cli

# Find OAuth-related code
grep -r "oauth" /tmp/gemini-cli --include="*.ts" --include="*.js" -l
grep -r "client_id" /tmp/gemini-cli --include="*.ts" --include="*.js" -A 2
grep -r "device" /tmp/gemini-cli --include="*.ts" --include="*.js" -A 5
grep -r "scope" /tmp/gemini-cli --include="*.ts" --include="*.js" -A 2
grep -r "generative" /tmp/gemini-cli --include="*.ts" --include="*.js" -A 2

# Clean up
rm -rf /tmp/gemini-cli
```

---

## Checklist

Fill in after running discovery:

### Client ID
- [ ] Found client ID: `_____________________`
- [ ] Is it a public client ID (safe to embed)? [ ] Yes [ ] No

### OAuth Endpoints
- [ ] Device code URL: `_____________________`
- [ ] Token URL: `_____________________`

### Scopes
- [ ] Required scopes: `_____________________`
- [ ] Are these the minimal scopes needed? [ ] Yes [ ] No [ ] Unknown

### Token Usage
- [ ] How is the access token used?
  - [ ] Bearer header: `Authorization: Bearer <token>`
  - [ ] Query parameter: `?key=<token>`
  - [ ] Other: `_____________________`

### Target API
- [ ] Which API endpoint does OAuth unlock?
  - [ ] `generativelanguage.googleapis.com` (Gemini API)
  - [ ] `aiplatform.googleapis.com` (Vertex AI)
  - [ ] Other: `_____________________`

### Refresh Token
- [ ] Does Google return refresh tokens? [ ] Yes [ ] No
- [ ] Refresh token grant type: `_____________________`

---

## Findings

*(To be filled in after discovery)*

### Client ID
```
TODO: Document the client ID found
```

### Scopes
```
TODO: Document the exact scopes used
```

### Token Usage Pattern
```
TODO: Document how the token is passed to APIs
```

### API Endpoint
```
TODO: Document which API endpoint is used
```

### Notes
```
TODO: Any additional observations
```

---

## Verification

After implementation, verify:

1. [ ] Device flow works end-to-end
2. [ ] Token is stored securely
3. [ ] Token refresh works
4. [ ] API calls succeed with OAuth token
5. [ ] Fallback to API key still works

---

## References

- Gemini CLI: https://github.com/google-gemini/gemini-cli
- Google OAuth device flow: https://developers.google.com/identity/protocols/oauth2/limited-input-device
- Gemini API: https://ai.google.dev/api
