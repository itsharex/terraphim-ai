use persistence::error;
use terraphim_config::{ConfigState, Role};
use terraphim_middleware::thesaurus::build_thesaurus_from_haystack;
use terraphim_types::{Document, IndexedDocument, RelevanceFunction, SearchQuery};

mod score;

#[derive(thiserror::Error, Debug)]
pub enum ServiceError {
    #[error("An error occurred: {0}")]
    Middleware(#[from] terraphim_middleware::Error),

    #[error("OpenDal error: {0}")]
    OpenDal(#[from] opendal::Error),

    #[error("Persistence error: {0}")]
    Persistence(#[from] persistence::Error),

    #[error("Config error: {0}")]
    Config(String),
}

pub type Result<T> = std::result::Result<T, ServiceError>;

pub struct TerraphimService {
    config_state: ConfigState,
}

impl<'a> TerraphimService {
    /// Create a new TerraphimService
    pub fn new(config_state: ConfigState) -> Self {
        Self { config_state }
    }

    /// Build a thesaurus from the haystack and update the knowledge graph automata URL
    async fn build_thesaurus(&self, search_query: &SearchQuery) -> Result<()> {
        Ok(build_thesaurus_from_haystack(self.config_state.clone(), search_query).await?)
    }

    /// Create document
    pub async fn create_document(&mut self, document: Document) -> Result<Document> {
        self.config_state.add_to_roles(&document).await?;
        Ok(document)
    }

    /// Get the role for the given search query
    async fn get_search_role(&self, search_query: &SearchQuery) -> Result<Role> {
        let search_role = search_query.role.clone().unwrap_or_default();
        let Some(role) = self.config_state.get_role(&search_role).await else {
            return Err(ServiceError::Config(format!(
                "Role {} not found in config",
                search_role
            )));
        };
        Ok(role)
    }

    /// Search for documents in the haystacks
    pub async fn search_documents(&self, search_query: &SearchQuery) -> Result<Vec<Document>> {
        // Get the role from the config
        log::debug!("Role for searching: {:?}", search_query.role);
        let role = self.get_search_role(search_query).await?;

        match role.relevance_function {
            RelevanceFunction::TitleScorer => {
                let index = terraphim_middleware::search_haystacks(
                    self.config_state.clone(),
                    search_query.clone(),
                )
                .await?;

                let indexed_docs: Vec<IndexedDocument> = self
                    .config_state
                    .search_indexed_documents(search_query)
                    .await;

                let documents = index.get_documents(indexed_docs);
                // Sort the documents by relevance
                let documents = score::sort_documents(search_query, documents);
                Ok(documents)
            }
            RelevanceFunction::TerraphimGraph => {
                self.build_thesaurus(search_query).await?;
                let indexed_docs: Vec<IndexedDocument> = self
                    .config_state
                    .search_indexed_documents(search_query)
                    .await;

                // TODO: Convert indexed documents to documents
                // We probably need to adjust the Thesaurus logseq haystack parser for this.
                // let documents: Vec<Document> = indexed_docs
                //     .iter()
                //     .map(|indexed_doc| indexed_doc.to_document())
                //     .collect();
                todo!()

                // Ok(documents)
            }
        }
    }

    /// Fetch the current config
    pub async fn fetch_config(&self) -> terraphim_config::Config {
        let current_config = self.config_state.config.lock().await;
        current_config.clone()
    }

    /// Update the config
    ///
    /// Overwrites the config in the config state and returns the updated
    /// config.
    pub async fn update_config(
        &self,
        config: terraphim_config::Config,
    ) -> Result<terraphim_config::Config> {
        let mut current_config = self.config_state.config.lock().await;
        *current_config = config.clone();
        Ok(config)
    }
}
