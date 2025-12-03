# Contributing to Rust Hedging Engine

Thank you for your interest in contributing! This document provides guidelines and instructions.

## Code of Conduct

Be respectful and constructive. We're all here to learn and improve.

## How to Contribute

### Reporting Bugs

1. Check existing issues first
2. Use the bug report template
3. Include:
    - Rust version
    - Operating system
    - Minimal reproduction code
    - Expected vs. actual behavior

### Suggesting Features

1. Check if already suggested
2. Explain the use case
3. Describe the expected API
4. Consider performance implications

### Pull Requests

1. Fork the repository
2. Create a feature branch
3. Write tests for new code
4. Ensure all tests pass
5. Run `cargo fmt` and `cargo clippy`
6. Update documentation
7. Submit PR with a clear description

## Development Setup

```bash
git clone https://github.com/maksat-software/hedging-engine
cd hedging-engine
cargo build
cargo test
```

## Testing Guidelines

- Write unit tests for new functions
- Add integration tests for workflows
- Include benchmarks for performance-critical code
- Aim for >80% code coverage

## Code Style

- Follow Rust conventions
- Use `cargo fmt` (enforced in CI)
- Fix all `cargo clippy` warnings
- Document public APIs
- Comment complex algorithms

## Performance Guidelines

- Hot path code must be allocation-free
- Prefer atomics to locks
- Benchmark performance-critical changes
- Document optimization rationale

## Documentation

- Add doc comments for public APIs
- Include examples in doc comments
- Update relevant docs/ files
- Keep README.md current

## Commit Messages

Format: `<type>(<scope>): <description>`

Types:

- feat: New feature
- fix: Bug fix
- docs: Documentation
- perf: Performance improvement
- test: Testing
- refactor: Code restructuring
- chore: Maintenance

Example:

```
feat(hedging): add gamma hedging strategy

Implements delta-gamma hedging for options portfolios.
Maintains sub-microsecond latency.
```

## Review Process

1. Automated CI checks must pass
2. Code review by maintainer
3. Performance regression check
4. Documentation review
5. Merge after approval

## Getting Help

- GitHub Discussions for questions
- GitHub Issues for bugs
- Email for private inquiries

## License

By contributing, you agree that your contributions will be licensed under MIT/Apache-2.0.