//! Project management tools.

use std::path::Path;

use tinymist_std::error::prelude::*;

use crate::project::*;

trait LockFileExt {
    fn declare(&mut self, args: &DocNewArgs) -> Id;
    fn preview(&mut self, doc_id: Id, args: &TaskPreviewArgs) -> Result<Id>;
    fn compile(&mut self, args: TaskCompileArgs) -> Result<Id>;
    fn export(&mut self, doc_id: Id, args: TaskCompileArgs) -> Result<Id>;
}

impl LockFileExt for LockFile {
    fn declare(&mut self, args: &DocNewArgs) -> Id {
        let id: Id = (&args.id).into();

        let root = args
            .root
            .as_ref()
            .map(|root| ResourcePath::from_user_sys(Path::new(root)));
        let main = ResourcePath::from_user_sys(Path::new(&args.id.input));

        let font_paths = args
            .font
            .font_paths
            .iter()
            .map(|p| ResourcePath::from_user_sys(p))
            .collect::<Vec<_>>();

        let package_path = args
            .package
            .package_path
            .as_ref()
            .map(|p| ResourcePath::from_user_sys(p));

        let package_cache_path = args
            .package
            .package_cache_path
            .as_ref()
            .map(|p| ResourcePath::from_user_sys(p));

        let input = ProjectInput {
            id: id.clone(),
            root,
            main: Some(main),
            font_paths,
            system_fonts: !args.font.ignore_system_fonts,
            package_path,
            package_cache_path,
        };

        self.replace_document(input);

        id
    }

    fn compile(&mut self, args: TaskCompileArgs) -> Result<Id> {
        let id = self.declare(&args.declare);
        self.export(id, args)
    }

    fn export(&mut self, doc_id: Id, args: TaskCompileArgs) -> Result<Id> {
        let task = args.to_task(doc_id)?;
        let task_id = task.id().clone();

        self.replace_task(task);

        Ok(task_id)
    }

    fn preview(&mut self, doc_id: Id, args: &TaskPreviewArgs) -> Result<Id> {
        let task_id = args
            .name
            .as_ref()
            .map(|t| Id::new(t.clone()))
            .unwrap_or(doc_id.clone());

        let when = args.when.unwrap_or(TaskWhen::OnType);
        let task = ProjectTask::Preview(PreviewTask {
            id: task_id.clone(),
            document: doc_id,
            when,
        });

        self.replace_task(task);

        Ok(task_id)
    }
}

/// Project document commands' main
pub fn project_main(args: DocCommands) -> Result<()> {
    LockFile::update(Path::new("."), |state| {
        match args {
            DocCommands::New(args) => {
                state.declare(&args);
            }
            DocCommands::Configure(args) => {
                let id: Id = (&args.id).into();

                state.route.push(ProjectRoute {
                    id: id.clone(),
                    priority: args.priority,
                });
            }
        }

        Ok(())
    })
}

/// Project task commands' main
pub fn task_main(args: TaskCommands) -> Result<()> {
    LockFile::update(Path::new("."), |state| {
        match args {
            TaskCommands::Compile(args) => {
                let _ = state.compile(args);
            }
            TaskCommands::Preview(args) => {
                let id = state.declare(&args.declare);
                let _ = state.preview(id, &args);
            }
        }

        Ok(())
    })
}
