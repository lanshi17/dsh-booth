import unittest
from domain import Account
from store import AccountStore
from service import LedgerService

class ExistingContract(unittest.TestCase):
    def test_transfer(self):
        store = AccountStore()
        store.add(Account("a", 10)); store.add(Account("b", 0))
        LedgerService(store).transfer("a", "b", 4)
        self.assertEqual((store.get("a").balance, store.get("b").balance), (6, 4))

if __name__ == "__main__":
    unittest.main()
