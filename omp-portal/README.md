# OMP portal frontend

This adapter uses the MIT-licensed [oh-my-pi](https://github.com/can1357/oh-my-pi)
frontend unchanged. It sends ordinary OMP input to dsh-booth's local portal
source and renders portal replies in the native OMP transcript. Team workers,
tickets, polling, and executor state remain backend details.

The backend currently requires a GitHub account and a repository it can push to.
Run `gh auth login`, then use `/booth-setup owner/repo` in the console. Until
that succeeds, the console explains why it cannot send instead of silently
queueing messages. The model used by the team is configured in DSH.

The upstream MIT notice is preserved in [`LICENSE`](LICENSE).
