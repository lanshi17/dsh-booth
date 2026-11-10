class AccountStore:
    def __init__(self):
        self._accounts = {}

    def add(self, account):
        if account.key in self._accounts:
            raise ValueError("duplicate account")
        self._accounts[account.key] = account

    def get(self, key):
        return self._accounts[key]
