use anyhow::{anyhow, Result};

use dbus::{
    arg::{RefArg, Variant},
    blocking::LocalConnection,
    tree::Factory,
};

use std::collections::HashMap;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::Duration;

use crate::library::Library;

mod cli;
mod search;

use cli::LbrCli;
use search::OrgGnomeShellSearchProvider2;

pub(crate) fn serve(lib: Library) -> Result<()> {
    let mut conn = LocalConnection::new_session()?;
    conn.request_name("lbr.server", false, true, false)?;

    let lib = LibrarySync::new(lib);

    let fact = Factory::new_fn::<()>();

    let search_lib = lib.clone();
    let search_interface =
        search::org_gnome_shell_search_provider2_server(&fact, (), move |_| search_lib.clone());
    let search_path = fact
        .object_path("/lbr/server/search", ())
        .add(search_interface);

    let cli_lib = lib.clone();
    let cli_interface = cli::lbr_cli_server(&fact, (), move |_| cli_lib.clone());
    let cli_path = fact.object_path("/lbr/server/cli", ()).add(cli_interface);

    fact.tree(())
        .add(search_path)
        .add(cli_path)
        .start_receive(&conn);

    loop {
        conn.process(Duration::from_millis(1000))?;
    }
}

#[derive(Clone)]
struct LibrarySync {
    lib: Arc<RwLock<Library>>,
}

impl LibrarySync {
    fn new(lib: Library) -> Self {
        LibrarySync {
            lib: Arc::new(RwLock::new(lib)),
        }
    }

    fn read(&self) -> Result<RwLockReadGuard<Library>> {
        self.lib
            .read()
            .map_err(|e| anyhow!("Failed to acquire read lock: {}", e))
    }

    fn write(&self) -> Result<RwLockWriteGuard<Library>> {
        self.lib
            .write()
            .map_err(|e| anyhow!("Failed to acquire write lock: {}", e))
    }
}

impl OrgGnomeShellSearchProvider2 for LibrarySync {
    fn get_initial_result_set(&self, terms: Vec<&str>) -> Result<Vec<String>> {
        let query = terms.join(" ");
        Ok(self
            .read()?
            .search(&query)
            .map(|id| id.to_string())
            .collect())
    }

    fn get_subsearch_result_set(
        &self,
        _previous_results: Vec<&str>,
        terms: Vec<&str>,
    ) -> Result<Vec<String>> {
        self.get_initial_result_set(terms)
    }

    fn get_result_metas(
        &self,
        identifiers: Vec<&str>,
    ) -> Result<Vec<HashMap<String, Variant<Box<dyn RefArg + 'static>>>>> {
        let lib = self.read()?;

        let mut metas = Vec::default();

        for identifier in identifiers {
            let id = identifier.parse()?;
            let doc = lib.get(id)?;

            let id: Variant<Box<dyn RefArg + 'static>> = Variant(Box::new(id.to_string()));
            let name: Variant<Box<dyn RefArg + 'static>> = Variant(Box::new(doc.title.clone()));
            let desc: Variant<Box<dyn RefArg + 'static>> =
                Variant(Box::new(doc.authors.clone().join(", ")));

            let mut meta = HashMap::default();
            meta.insert("id".to_string(), id);
            meta.insert("name".to_string(), name);
            meta.insert("description".to_string(), desc);

            metas.push(meta);
        }

        Ok(metas)
    }

    fn activate_result(&self, identifier: &str, _terms: Vec<&str>, _timestamp: u32) -> Result<()> {
        let id = identifier.parse()?;
        self.read()?.open(id)
    }
    fn launch_search(&self, _terms: Vec<&str>, _timestamp: u32) -> Result<()> {
        Ok(())
    }
}

impl AsRef<dyn OrgGnomeShellSearchProvider2 + 'static> for LibrarySync {
    fn as_ref(&self) -> &(dyn OrgGnomeShellSearchProvider2 + 'static) {
        self
    }
}

impl LbrCli for LibrarySync {
    fn insert(&self, doc: &str, path: &str) -> Result<()> {
        let doc = serde_json::from_str(doc)?;
        self.write()?.insert(doc, path)
    }
}

impl AsRef<dyn LbrCli + 'static> for LibrarySync {
    fn as_ref(&self) -> &(dyn LbrCli + 'static) {
        self
    }
}