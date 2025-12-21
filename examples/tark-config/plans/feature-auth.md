# Plan: User Authentication Feature

## Overview
Add user authentication to the application using JWT tokens.

## Tasks

### 1. Database Schema
- [ ] Create `users` table with email, password_hash, created_at
- [ ] Create `sessions` table for refresh tokens
- [ ] Add migration files

### 2. API Endpoints
- [ ] POST /auth/register - User registration
- [ ] POST /auth/login - User login
- [ ] POST /auth/logout - User logout
- [ ] POST /auth/refresh - Refresh JWT token
- [ ] GET /auth/me - Get current user

### 3. Middleware
- [ ] JWT validation middleware
- [ ] Rate limiting for auth endpoints

### 4. Frontend
- [ ] Login form component
- [ ] Registration form component
- [ ] Auth context provider
- [ ] Protected route wrapper

## Files to Modify
- `src/routes/auth.rs`
- `src/middleware/auth.rs`
- `src/models/user.rs`
- `frontend/src/contexts/AuthContext.tsx`

## Dependencies
- `jsonwebtoken` for JWT handling
- `argon2` for password hashing
- `validator` for input validation

## Notes
- Use httpOnly cookies for refresh tokens
- Implement CSRF protection
- Add 2FA support in future iteration

