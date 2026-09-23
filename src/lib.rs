pub mod analysis;
pub mod app;
pub mod editor;
pub mod file_engine;
pub mod formats;
pub mod indexing;
pub mod search;

pub use analysis::{AnalysisProgress, AnalysisReport, FieldStats, InferredType, StreamingFieldAnalyzer, ValueFrequency};
pub use app::UltraViewerApp;
pub use editor::{EditOperation, EditorDocument, PieceTable, PieceTableReader, SaveManager, UndoStack, Viewport};
pub use file_engine::{Encoding, FileEngine, LineIndex};
pub use formats::{FileType, FormatDetector};
pub use indexing::LineIndexer;

