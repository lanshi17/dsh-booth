class LedgerService:
    def __init__(self, store):
        self.store = store

    def transfer(self, source, target, amount):
        src = self.store.get(source)
        dst = self.store.get(target)
        if amount <= 0 or src.balance < amount:
            raise ValueError("invalid transfer")
        src.balance -= amount
        dst.balance += amount
