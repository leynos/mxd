# Implementation Roadmap

This document tracks the major features required for a full Hotline-inspired BBS
server. Features are grouped by domain with checkboxes. Items that have already
been implemented in the repository are checked off.

## Core Server

- [x] Async TCP server using Tokio
- [x] Handshake and transaction framing
- [x] User login with Argon2 password verification
- [ ] User presence tracking and session management
- [ ] Administrative commands (kick, ban, shut down)

## User Management

- [x] Create users via CLI (`create-user` subcommand)
- [x] Store users in SQLite via Diesel migrations=
- [ ] Account editing
- [ ] Account deletion
- [ ] Permission and role system

## News System

- [x] List news categories and bundles
  - [ ] Hierarchy support
- [x] List article titles
- [x] Retrieve article data
- [ ] Post new articles
- [ ] Post replies
- [ ] Edit articles
- [ ] Delete articles

## File Sharing

- [x] List accessible files for a user
- [ ] Upload files
  - [ ] Resume support
- [ ] Download files
  - [ ] Resume support
- [ ] Folder management (create, move, delete)
- [ ] File comments and metadata editing
- [ ] Object storage backend using `object_store`

## Chat and Messaging

- [ ] Create chat rooms
- [ ] Join chat rooms
- [ ] Invite to chat room
- [ ] Send public and private messages
- [ ] Presence notifications

## Compatibility and Testing

- [x] Integration validator crate using `hx`
- [x] Fuzzing harness with AFL++
  - [ ] Regular execution of fuzzing
- [ ] Full protocol coverage in tests

## Database Backends

- [x] SQLite support with migrations
- [ ] PostgreSQL backend support *(in progress)*

The roadmap will evolve as the project grows. Checked items indicate
functionality present in the current code base. Unchecked items highlight areas
for future development.
