def add(a, b):
    return a + b


def _helper():
    pass


class Point:
    def __init__(self, x, y):
        self.x = x
        self.y = y

    def _private(self):
        pass

    def __repr__(self):
        return "Point"

    def __hidden(self):
        pass


def caller():
    add(1, 2)
    p = Point(1, 2)
    p._private()
