import csv
from decimal import Decimal
def total(rows):
    return sum(Decimal(r["amount"]) for r in rows if r["amount"])
