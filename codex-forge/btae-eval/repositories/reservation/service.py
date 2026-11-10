from domain import Booking

class BookingService:
    def __init__(self, store):
        self.store = store

    def book(self, resource, start, end):
        if start >= end:
            raise ValueError("empty interval")
        for old in self.store.for_resource(resource):
            if start < old.end and old.start < end:
                raise ValueError("overlap")
        booking = Booking(resource, start, end)
        self.store.add(booking)
        return booking
