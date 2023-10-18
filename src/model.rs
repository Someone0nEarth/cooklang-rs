//! Recipe representation

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::{
    ast::Modifiers,
    convert::Converter,
    metadata::Metadata,
    quantity::{Quantity, QuantityAddError, QuantityValue, ScalableValue, ScaledQuantity},
    GroupedQuantity, Value,
};

/// A complete recipe
///
/// The recipes does not have a name. You give it externally or maybe use
/// some metadata key.
///
/// The recipe returned from parsing is a [`ScalableRecipe`].
///
/// The difference between [`ScalableRecipe`] and [`ScaledRecipe`] is in the
/// values of the quantities of ingredients, cookware and timers. The parser
/// returns [`ScalableValue`]s and after scaling, these are converted to regular
/// [`Value`]s.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct Recipe<D, V: QuantityValue> {
    /// Metadata
    pub metadata: Metadata,
    /// Each of the sections
    ///
    /// If no sections declared, a section without name
    /// is the default.
    pub sections: Vec<Section>,
    /// All the ingredients
    pub ingredients: Vec<Ingredient<V>>,
    /// All the cookware
    pub cookware: Vec<Cookware<V>>,
    /// All the timers
    pub timers: Vec<Timer<V>>,
    /// All the inline quantities
    pub inline_quantities: Vec<ScaledQuantity>,
    pub(crate) data: D,
}

/// A recipe before being scaled
///
/// Note that this doesn't implement [`Recipe::convert`]. Only scaled recipes
/// can be converted.
pub type ScalableRecipe = Recipe<(), ScalableValue>;

/// A recipe after being scaled
///
/// Note that this doesn't implement [`Recipe::scale`]. A recipe can only be
/// scaled once.
pub type ScaledRecipe = Recipe<crate::scale::Scaled, Value>;

/// A section holding steps
#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Clone)]
pub struct Section {
    /// Name of the section
    pub name: Option<String>,
    /// Content inside
    pub content: Vec<Content>,
}

impl Section {
    pub(crate) fn new(name: Option<String>) -> Section {
        Self {
            name,
            content: Vec::new(),
        }
    }

    /// Check if the section is empty
    ///
    /// A section is empty when it has no name and no content.
    pub fn is_empty(&self) -> bool {
        self.name.is_none() && self.content.is_empty()
    }
}

/// Each type of content inside a section
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum Content {
    /// A step
    Step(Step),
    /// A paragraph of just text, no instructions
    Text(String),
}

impl Content {
    /// Checks if the content is a regular step
    pub fn is_step(&self) -> bool {
        matches!(self, Self::Step(_))
    }

    /// Checks if the content is a text paragraph
    pub fn is_text(&self) -> bool {
        matches!(self, Self::Text(_))
    }

    /// Get's the inner step
    ///
    /// # Panics
    /// If the content is [`Content::Text`]
    pub fn as_step(&self) -> &Step {
        match self {
            Content::Step(s) => s,
            Content::Text(_) => panic!("content is text"),
        }
    }

    /// Get's the inner step
    ///
    /// # Panics
    /// If the content is [`Content::Step`]
    pub fn as_text(&self) -> &str {
        match self {
            Content::Step(_) => panic!("content is step"),
            Content::Text(t) => t.as_str(),
        }
    }
}

/// A step holding step [`Item`]s
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[non_exhaustive]
pub struct Step {
    /// [`Item`]s inside
    pub items: Vec<Item>,

    /// Step number
    ///
    /// The step numbers start at 1 in each section and increase with non
    /// text step.
    pub number: u32,
}

/// A step item
///
/// Except for [`Item::Text`], the value is the index where the item is located
/// in it's corresponding [`Vec`] in the [`Recipe`].
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Item {
    /// Just plain text
    Text {
        value: String,
    },
    Ingredient {
        index: usize,
    },
    Cookware {
        index: usize,
    },
    Timer {
        index: usize,
    },
    InlineQuantity {
        index: usize,
    },
}

/// A recipe ingredient
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct Ingredient<V: QuantityValue = Value> {
    /// Name
    ///
    /// This can have the form of a path if the ingredient references a recipe.
    pub name: String,
    /// Alias
    pub alias: Option<String>,
    /// Quantity
    pub quantity: Option<Quantity<V>>,
    /// Note
    pub note: Option<String>,
    /// How the cookware is related to others
    pub relation: IngredientRelation,
    pub(crate) modifiers: Modifiers,
}

impl<V: QuantityValue> Ingredient<V> {
    /// Gets the name the ingredient should be displayed with
    pub fn display_name(&self) -> Cow<str> {
        let mut name = Cow::from(&self.name);
        if self.modifiers.contains(Modifiers::RECIPE) {
            if let Some(recipe_name) = std::path::Path::new(&self.name)
                .file_stem()
                .and_then(|s| s.to_str())
            {
                name = recipe_name.into();
            }
        }
        self.alias.as_ref().map(Cow::from).unwrap_or(name)
    }

    /// Access the ingredient modifiers
    pub fn modifiers(&self) -> Modifiers {
        self.modifiers
    }
}

impl Ingredient<Value> {
    /// Calculates the total quantity adding all the quantities from the
    /// references.
    pub fn total_quantity<'a>(
        &'a self,
        all_ingredients: &'a [Self],
        converter: &Converter,
    ) -> Result<Option<ScaledQuantity>, QuantityAddError> {
        let mut quantities = self.all_quantities(all_ingredients);

        let Some(mut total) = quantities.next().cloned() else {
            return Ok(None);
        };
        for q in quantities {
            total = total.try_add(q, converter)?;
        }
        let _ = total.fit(converter);

        Ok(Some(total))
    }

    /// Groups all quantities from itself and it's references (if any).
    /// ```
    /// # use cooklang::{CooklangParser, Extensions, Converter, TotalQuantity, Value, Quantity};
    /// let parser = CooklangParser::new(Extensions::all(), Converter::bundled());
    /// let recipe = parser.parse("@flour{1000%g} @&flour{100%g}")
    ///                 .into_output()
    ///                 .unwrap()
    ///                 .default_scale();
    ///
    /// let flour = &recipe.ingredients[0];
    /// assert_eq!(flour.name, "flour");
    ///
    /// let grouped_flour = flour.group_quantities(
    ///                         &recipe.ingredients,
    ///                         parser.converter()
    ///                     );
    ///
    /// assert_eq!(
    ///     grouped_flour.total(),
    ///     TotalQuantity::Single(
    ///         Quantity::new(
    ///             Value::from(1.1),
    ///             Some("kg".to_string()) // Unit fit to kilograms
    ///         )
    ///     )
    /// );
    /// ```
    pub fn group_quantities(
        &self,
        all_ingredients: &[Self],
        converter: &Converter,
    ) -> GroupedQuantity {
        let mut grouped = GroupedQuantity::default();
        for q in self.all_quantities(all_ingredients) {
            grouped.add(q, converter);
        }
        let _ = grouped.fit(converter);
        grouped
    }

    /// Gets an iterator over all quantities of this ingredient and its references.
    pub fn all_quantities<'a>(
        &'a self,
        all_ingredients: &'a [Self],
    ) -> impl Iterator<Item = &ScaledQuantity> {
        std::iter::once(self.quantity.as_ref())
            .chain(
                self.relation
                    .referenced_from()
                    .iter()
                    .copied()
                    .map(|i| all_ingredients[i].quantity.as_ref()),
            )
            .flatten()
    }
}

/// A recipe cookware item
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct Cookware<V: QuantityValue = Value> {
    /// Name
    pub name: String,
    /// Alias
    pub alias: Option<String>,
    /// Amount needed
    ///
    /// Note that this is a value, not a quantity, so it doesn't have units.
    pub quantity: Option<V>,
    /// Note
    pub note: Option<String>,
    /// How the cookware is related to others
    pub relation: ComponentRelation,
    pub(crate) modifiers: Modifiers,
}

impl<V: QuantityValue> Cookware<V> {
    /// Gets the name the cookware item should be displayed with
    pub fn display_name(&self) -> &str {
        self.alias.as_ref().unwrap_or(&self.name)
    }

    /// Access the cookware modifiers
    pub fn modifiers(&self) -> Modifiers {
        self.modifiers
    }
}

impl Cookware<Value> {
    /// Groups all the amounts of itself and it's references
    ///
    /// The first element is a grouped numeric value (if any), the rest are text
    /// values.
    ///
    /// ```
    /// # use cooklang::{CooklangParser, Extensions, Converter, TotalQuantity, Value, Quantity};
    /// let parser = CooklangParser::new(Extensions::all(), Converter::bundled());
    /// let recipe = parser.parse("#pan{3} #&pan{1} #&pan{big}")
    ///                 .into_output()
    ///                 .unwrap()
    ///                 .default_scale();
    ///
    /// let pan = &recipe.cookware[0];
    /// assert_eq!(pan.name, "pan");
    ///
    /// let grouped_pans = pan.group_amounts(&recipe.cookware);
    ///
    /// assert_eq!(
    ///     grouped_pans,
    ///     vec![
    ///         Value::from(4.0),
    ///         Value::from("big".to_string()),
    ///     ]
    /// );
    /// ```
    pub fn group_amounts(&self, all_cookware: &[Self]) -> Vec<Value> {
        use crate::quantity::TryAdd;

        let mut amounts = self.all_amounts(all_cookware);
        let mut r = Vec::new();
        loop {
            match amounts.next() {
                Some(v) if v.is_text() => r.push(v.clone()),
                Some(v) => {
                    r.insert(0, v.clone());
                    break;
                }
                None => return r,
            }
        }
        for v in amounts {
            if v.is_text() {
                r.push(v.clone());
            } else {
                r[0] = r[0]
                    .try_add(v)
                    .expect("non text to non text value add error");
            }
        }
        r
    }

    /// Gets an iterator over all quantities of this ingredient and its references.
    pub fn all_amounts<'a>(&'a self, all_cookware: &'a [Self]) -> impl Iterator<Item = &Value> {
        std::iter::once(self.quantity.as_ref())
            .chain(
                self.relation
                    .referenced_from()
                    .iter()
                    .copied()
                    .map(|i| all_cookware[i].quantity.as_ref()),
            )
            .flatten()
    }
}

/// Relation between components
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ComponentRelation {
    /// The component is a definition
    Definition {
        /// List of indices of other components of the same kind referencing this
        /// one
        referenced_from: Vec<usize>,
        /// True if the definition was in a step
        ///
        /// This is only false for components defined in components mode.
        defined_in_step: bool,
    },
    /// The component is a reference
    Reference {
        /// Index of the definition component
        references_to: usize,
    },
}

impl ComponentRelation {
    /// Gets a list of the components referencing this one.
    ///
    /// Returns a list of indices to the corresponding vec in [`Recipe`].
    pub fn referenced_from(&self) -> &[usize] {
        match self {
            ComponentRelation::Definition {
                referenced_from, ..
            } => referenced_from,
            ComponentRelation::Reference { .. } => &[],
        }
    }

    /// Get the index the relations references to
    pub fn references_to(&self) -> Option<usize> {
        match self {
            ComponentRelation::Definition { .. } => None,
            ComponentRelation::Reference { references_to } => Some(*references_to),
        }
    }

    /// Check if the relation is a reference
    pub fn is_reference(&self) -> bool {
        matches!(self, ComponentRelation::Reference { .. })
    }

    /// Check if the relation is a definition
    pub fn is_definition(&self) -> bool {
        matches!(self, ComponentRelation::Definition { .. })
    }

    /// Checks if the definition was in a step
    ///
    /// Returns None for references
    pub fn is_defined_in_step(&self) -> Option<bool> {
        match self {
            ComponentRelation::Definition {
                defined_in_step, ..
            } => Some(*defined_in_step),
            ComponentRelation::Reference { .. } => None,
        }
    }
}

/// Same as [`ComponentRelation`] but with the ability to reference steps and
/// sections apart from other ingredients.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct IngredientRelation {
    #[serde(flatten)]
    relation: ComponentRelation,
    reference_target: Option<IngredientReferenceTarget>,
}

/// Target an ingredient reference references to
///
/// This is obtained from [`IngredientRelation::references_to`]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub enum IngredientReferenceTarget {
    /// Ingredient definition
    Ingredient,
    /// Step in the current section
    Step,
    /// Section in the current recipe
    Section,
}

impl IngredientRelation {
    pub(crate) fn definition(referenced_from: Vec<usize>, defined_in_step: bool) -> Self {
        Self {
            relation: ComponentRelation::Definition {
                referenced_from,
                defined_in_step,
            },
            reference_target: None,
        }
    }

    pub(crate) fn reference(
        references_to: usize,
        reference_target: IngredientReferenceTarget,
    ) -> Self {
        Self {
            relation: ComponentRelation::Reference { references_to },
            reference_target: Some(reference_target),
        }
    }

    /// Gets a list of the components referencing this one.
    ///
    /// Returns a list of indices to the corresponding vec in [Recipe].
    pub fn referenced_from(&self) -> &[usize] {
        self.relation.referenced_from()
    }

    pub(crate) fn referenced_from_mut(&mut self) -> Option<&mut Vec<usize>> {
        match &mut self.relation {
            ComponentRelation::Definition {
                referenced_from, ..
            } => Some(referenced_from),
            ComponentRelation::Reference { .. } => None,
        }
    }

    /// Get the index the relation refrences to and the target
    ///
    /// The first element of the tuple is an index into:
    ///
    /// | Target | Where |
    /// |--------|-------|
    /// | [`Ingredient`] | [`Recipe::ingredients`] |
    /// | [`Step`] | [`Section::content`] in the same section this ingredient is. It's guaranteed that the content is a step. |
    /// | [`Section`] | [`Recipe::sections`] |
    ///
    /// [`Ingredient`]: IngredientReferenceTarget::Ingredient
    /// [`Step`]: IngredientReferenceTarget::Step
    /// [`Section`]: IngredientReferenceTarget::Section
    ///
    /// If the [`INTERMEDIATE_PREPARATIONS`](crate::Extensions::INTERMEDIATE_PREPARATIONS)
    /// extension is disabled, the target will always be
    /// [`IngredientReferenceTarget::Ingredient`].
    pub fn references_to(&self) -> Option<(usize, IngredientReferenceTarget)> {
        self.relation
            .references_to()
            .map(|index| (index, self.reference_target.unwrap()))
    }

    /// Checks if the relation is a regular reference to an ingredient
    pub fn is_regular_reference(&self) -> bool {
        use IngredientReferenceTarget::*;
        self.references_to()
            .map(|(_, target)| target == Ingredient)
            .unwrap_or(false)
    }

    /// Checks if the relation is an intermediate reference to a step or section
    pub fn is_intermediate_reference(&self) -> bool {
        use IngredientReferenceTarget::*;
        self.references_to()
            .map(|(_, target)| matches!(target, Step | Section))
            .unwrap_or(false)
    }

    /// Check if the relation is a definition
    pub fn is_definition(&self) -> bool {
        self.relation.is_definition()
    }

    /// Checks if the definition was in a step
    ///
    /// Returns None for references
    pub fn is_defined_in_step(&self) -> Option<bool> {
        self.relation.is_defined_in_step()
    }
}

/// A recipe timer
///
/// If created from parsing, at least one of the fields is guaranteed to be
/// [`Some`].
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct Timer<V: QuantityValue = Value> {
    /// Name
    pub name: Option<String>,
    /// Time quantity
    ///
    /// If created from parsing the following applies:
    ///
    /// - If the [`ADVANCED_UNITS`](crate::Extensions::ADVANCED_UNITS) extension
    /// is enabled, this is guaranteed to have a time unit.
    ///
    /// - If the [`TIMER_REQUIRES_TIME`](crate::Extensions::TIMER_REQUIRES_TIME)
    /// extension is enabled, this is guaranteed to be [`Some`].
    pub quantity: Option<Quantity<V>>,
}
