use crate::actions::Recipe;

/// Checks the arguments of a call before `just` sees them. `just` reads arguments beyond a
/// recipe's parameters as further recipe names, which would run recipes nobody approved (or
/// private ones), so those are refused.
///
/// With `strict` (a caller that is not an agent session: an app over HTTP) two more things are
/// refused:
/// - Options: an argument starting with `-` (a recipe with `{{name}}`: any word of it), so a call
///   can't turn into `rm --version` or `find data -name x -delete`. Recipes don't have to use `--`,
///   and an argument that really starts with `-` can't come through this route.
/// - Shell syntax, when the recipe pastes arguments into the shell as `{{name}}`. Read from the
///   justfile dump, this can't tell `{{ quote(name) }}` (safe) from the rest, so it is refused too.
///
/// This stops an argument from changing the command around it, nothing more: a recipe that runs its
/// argument as a command (`sh cmd: {{cmd}}`) still does. Only approving a recipe for apps covers
/// that, see [`Approvals::may_run`](crate::actions::Approvals::may_run).
pub fn check_args(recipe: &Recipe, args: &[String], strict: bool) -> Result<(), String> {
    if !recipe.variadic && args.len() > recipe.parameter_count {
        return Err(format!(
            "`{}` takes {} argument(s), got {}",
            recipe.name,
            recipe.parameter_count,
            args.len()
        ));
    }
    if !strict {
        return Ok(());
    }
    let is_option = |arg: &&String| {
        if recipe.interpolates_parameters {
            arg.split_whitespace().any(|word| word.starts_with('-'))
        } else {
            arg.starts_with('-')
        }
    };
    if let Some(arg) = args.iter().find(is_option) {
        return Err(format!(
            "{arg:?} would reach `{}` as a command-line option, so apps can't pass it: no argument \
             (or word of an argument pasted as {{{{name}}}}) may start with `-`",
            recipe.name
        ));
    }
    if recipe.interpolates_parameters
        && let Some(arg) = args.iter().find(|a| !a.chars().all(plain))
    {
        return Err(format!(
            "`{}` pastes its arguments into a shell command, and {arg:?} has characters the shell \
             reads as syntax; use only letters, digits, spaces and _-.,:/@+=%~ (or have the recipe \
             read them as \"$1\" with [positional-arguments]; {{{{ quote(name) }}}} counts as pasting)",
            recipe.name
        ));
    }
    Ok(())
}

fn plain(c: char) -> bool {
    c.is_alphanumeric() || " _-.,:/@+=%~".contains(c)
}
