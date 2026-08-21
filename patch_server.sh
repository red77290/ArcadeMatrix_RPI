#!/bin/bash
cat << 'INNER_EOF' >> src/api/server.rs

#[get("/api/engines")]
async fn api_engines() -> impl Responder {
    let descriptors = crate::core::registry::EngineRegistry::get_all_descriptors();
    HttpResponse::Ok().json(descriptors)
}
INNER_EOF
sed -i.bak 's/\.service(api_sprites_playlists)/\.service(api_engines)\n            \.service(api_sprites_playlists)/g' src/api/server.rs
rm src/api/server.rs.bak
