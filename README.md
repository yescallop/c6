# c6

This is a TUI application in which you can play [Connect6](https://en.wikipedia.org/wiki/Connect6) games offline, as well as saving and loading game records.

## Usage

To start a game on an infinite board, run the executable with no arguments.

To load a game, provide the path to the record file as the only argument.

### Key bindings

```text
Up/Left/Down/Right: Move the cursor.
Space/Enter: Make a move.
P: Pass.
C: Reset the cursor to the origin.
[: Undo last move.
]: Redo the next move.
Home: Jump to the first move.
End: Jump to the last move.
S: Save the game.
Q: Quit if the game is saved.
Ctrl+C: Force quit.
```

### Known limitations

- No messages are displayed after you save a game. If you can press `Q` to quit, then it's saved. Also you can't save to a path other than `save.c6`.
- The only way to start a game on a bounded board is to load a record file with the correct `Board` header and press `Home` (if needed).

## License

This project is licensed under the [MIT License](/LICENSE).
