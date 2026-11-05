use crate::actions::Recipe;

/// Checks the arguments of a call before `just` sees them. `just` reads arguments beyond a
/// recipe's parameters as further recipe names, which would run recipes nobody approved (or
/// private ones), so those are refused. With `strict` (a caller that is not an agent session: an
/// app over HTTP), arguments with shell metacharacters are refused too when the recipe pastes
/// them into the shell as `{{name}}`.
pub fn check_args(recipe: &Recipe, args: &[String], strict: bool) -> Result<(), String> {
    if !recipe.variadic && args.len() > recipe.parameter_count {
        return Err(format!(
            "`{}` takes {} argument(s), got {}",
            recipe.name,
            recipe.parameter_count,
            args.len()
        ));
    }
    if strict
        && recipe.interpolates_parameters
        && let Some(arg) = args.iter().find(|a| !a.chars().all(plain))
    {
        return Err(format!(
            "`{}` pastes its arguments into a shell command, and {arg:?} has characters the shell \
             reads as syntax; use only letters, digits, spaces and _-.,:/@+=%~ (or have the recipe \
             read them as \"$1\" with [positional-arguments])",
            recipe.name
        ));
    }
    Ok(())
}

fn plain(c: char) -> bool {
    c.is_alphanumeric() || " _-.,:/@+=%~".contains(c)
}
