# Build/test conventions for a morloc-dungeon example.
#
#   make build   build the three programs
#   make test    run them and diff against test/exp.txt
#   make clean   remove build products

EXE := pacman
TEST := pacman-test
RENDER := pacman-tui-test

.PHONY: build test clean

build:
	morloc make -o $(EXE) main.loc
	morloc make -o $(TEST) test.loc
	morloc make -o $(RENDER) tui-test.loc

test: build
	: > test/obs.txt
	./$(TEST) test >> test/obs.txt
	rm -f test/rt.sav
	./$(TEST) writeReplay test/rt.sav
	./$(TEST) -f jsonl showSave test/rt.sav >> test/obs.txt
	./$(RENDER) -f jsonl @ 70 40 >> test/obs.txt
	diff -u test/exp.txt test/obs.txt

clean:
	rm -rf $(EXE) $(EXE)-build $(TEST) $(TEST)-build $(RENDER) $(RENDER)-build
	rm -f test/obs.txt test/rt.sav pacman.sav
