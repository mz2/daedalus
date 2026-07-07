"""Tiny fixture the E2E agent must fix (issue #19): add() has an obvious bug."""


def add(a, b):
    return a - b


if __name__ == "__main__":
    print(add(3, 4))
