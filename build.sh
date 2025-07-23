napi build --platform --release $@
napi build --platform --target x86_64-unknown-linux-musl --release $@
echo -e "import {\n\tConnectionOptions,\n\tsurrealdbNodeEngines as embeddedSurrealdbNodeEngines,\n} from \"./lib-src/embedded.js\";\n$(cat index.d.ts)" > index.d.ts
echo -e '\nexport declare const surrealdbNodeEngines: typeof embeddedSurrealdbNodeEngines' >> index.d.ts
mkdir -p artifacts
mv surrealdb.node.*.node artifacts/
node move_artifacts.js
