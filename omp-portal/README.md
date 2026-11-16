# Booth portal frontend

This adapter uses the MIT-licensed [oh-my-pi](https://github.com/can1357/oh-my-pi)
interaction kernel unchanged. It sends ordinary booth input to dsh-booth's local portal
source and renders portal replies in the booth transcript. Team workers,
tickets, polling, and executor state remain backend details.

The backend currently requires a GitHub account and a repository it can push to.
When booth starts inside a Git checkout, it reads the GitHub `origin` and runs
setup automatically. Use `/booth-setup` to retry, or `/booth-setup owner/repo`
to override it. Until GitHub access succeeds, the console explains why it cannot
send instead of silently queueing messages. The model used by the team is configured in DSH.

The upstream MIT notice is preserved in [`LICENSE`](LICENSE).
