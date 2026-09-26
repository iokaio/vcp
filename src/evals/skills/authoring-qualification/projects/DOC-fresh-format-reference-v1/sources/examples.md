# Maintainer examples

This is valid input under revision 7:

```csv
item_id,label,quantity,location
KIT-4,"Cable, blue",0,reserve
BOX-2,Storage box,12,
```

`KIT-4` and `kit-4` are different strings, but the lowercase form is not a valid ID. Duplicate `KIT-4` rows in one file are invalid. `1.5` is not a valid quantity. An empty label is invalid even when it consists of spaces. A blank location is valid.

An earlier discussion suggested defaulting a blank location to north; revision 7 explicitly rejected that suggestion. The example's blank value intentionally remains unassigned.
