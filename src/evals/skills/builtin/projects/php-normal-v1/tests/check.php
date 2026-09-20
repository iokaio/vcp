<?php
require __DIR__."/../src/Amount.php";
if (Fixture\Amount::parse("42") !== 42) { exit(1); }
