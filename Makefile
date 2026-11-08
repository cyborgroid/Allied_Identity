.PHONY: run build test clean reset

run:
	./AlliedID

build:
	dotnet build host/AlliedID/AlliedID.csproj -c Release -o .build --nologo -v quiet -clp:ErrorsOnly
	touch .build/.stamp

test:
	cargo test --manifest-path core/Cargo.toml
	dotnet build host/AlliedID/AlliedID.csproj -c Release --nologo -v quiet -warnaserror

clean:
	cargo clean --manifest-path core/Cargo.toml
	rm -rf .build host/AlliedID/bin host/AlliedID/obj

# 생성된 데이터와 배운 것·기억을 모두 지우고 처음 상태로 (되돌릴 수 없음)
reset:
	rm -rf data/generated data/state
