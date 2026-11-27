#!/bin/sh
# A protocol fixture. Its answers depend only on the position and requested line count.
printf '%s\n' "$$" > "$0.pid"
lines=1
position=
while IFS= read -r command; do
    printf '%s\n' "$command" >> "$0.commands"
    case "$command" in
        uci)
            if [ "${0##*/}" != single-line ]; then
                echo 'option name MultiPV type spin default 1 min 1 max 2'
            fi
            echo uciok ;;
        'setoption name MultiPV value 2') lines=2 ;;
        isready) echo readyok ;;
        position\ *) position=$command ;;
        'go infinite')
            # The window test can hold the next answer until it has seen the old rows clear.
            while [ -e "$0.hold" ]; do sleep 0.02; done
            case "$position" in
                'position startpos')
                    if [ "$lines" = 2 ]; then
                        echo 'info depth 8 multipv 1 score cp 34 pv e2e4 e7e5'
                        echo 'info depth 8 multipv 2 score cp 12 pv d2d4 d7d5'
                    else
                        echo 'info depth 8 score cp 34 pv e2e4 e7e5'
                    fi ;;
                'position startpos moves e2e4')
                    echo 'info depth 8 score cp 27 pv c7c5 g1f3' ;;
                'position fen 8/8/8/8/8/5kq1/8/7K b - - 0 1')
                    echo 'info depth 8 score mate 1 pv g3g2' ;;
            esac ;;
        stop)
            # Engines may finish reporting on the previous position after a stop.
            echo 'info depth 9 score cp 99 pv c7c5'
            echo 'bestmove (none)' ;;
        quit) exit 0 ;;
    esac
done
