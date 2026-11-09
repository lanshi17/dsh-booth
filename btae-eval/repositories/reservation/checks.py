import unittest
from store import BookingStore
from service import BookingService

class ExistingContract(unittest.TestCase):
    def test_adjacent(self):
        service = BookingService(BookingStore())
        service.book("a", 1, 2)
        service.book("a", 2, 3)
        with self.assertRaises(ValueError):
            service.book("a", 1, 3)

if __name__ == "__main__":
    unittest.main()
