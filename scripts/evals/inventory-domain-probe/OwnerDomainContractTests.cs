// SPDX-License-Identifier: Apache-2.0
// PROTECTED FILE: owner acceptance. Do not edit, rename, exclude or replace.
using System;
using System.Collections.Generic;
using System.ComponentModel.DataAnnotations;
using System.Linq;
using System.Reflection;
using Xunit;

namespace Inventory.Tests;

public sealed class OwnerDomainContractTests
{
    private static Type Movement()
    {
        var model = Assembly.Load("Inventory.Web").GetType("Inventory.Web.Data.StockMovement");
        Assert.True(model is { IsClass: true, IsAbstract: false }, "T1 requires Inventory.Web.Data.StockMovement.");
        return model!;
    }

    [Fact]
    public void Reason_is_exact_required_enum()
    {
        var reason = Movement().GetProperty("Reason");
        Assert.True(reason is { CanWrite: true } && reason.PropertyType.IsEnum,
            "Reason must be an enum, not a string or an unchecked surrogate.");
        Assert.Equal(new[] { "Adjustment", "Receipt", "Sale" }, Enum.GetNames(reason!.PropertyType).OrderBy(name => name, StringComparer.Ordinal).ToArray());
    }

    [Fact]
    public void Quantity_rejects_zero_and_accepts_nonzero_contract_examples()
    {
        var model = Movement();
        var quantity = model.GetProperty("Quantity");
        var reason = model.GetProperty("Reason");
        Assert.True(quantity is { CanWrite: true } && quantity.PropertyType == typeof(int), "Quantity must be an integer.");
        Assert.True(reason is { CanWrite: true } && reason.PropertyType.IsEnum, "Reason must be the required enum.");
        foreach (var sample in new[] { (0, "Receipt", false), (0, "Sale", false), (0, "Adjustment", false), (1, "Receipt", true), (-1, "Receipt", true), (1, "Sale", true), (-1, "Sale", true), (1, "Adjustment", true), (-1, "Adjustment", true) })
        {
            var instance = Activator.CreateInstance(model)!;
            Set(instance, "Id", 1); Set(instance, "ProductId", 1);
            quantity!.SetValue(instance, sample.Item1);
            reason!.SetValue(instance, Enum.Parse(reason.PropertyType, sample.Item2));
            Set(instance, "OccurredAt", DateTime.UtcNow); Set(instance, "Note", "Owner domain acceptance");
            var valid = Validator.TryValidateObject(instance, new ValidationContext(instance), new List<ValidationResult>(), true);
            Assert.True(valid == sample.Item3, $"Quantity {sample.Item1}, Reason {sample.Item2}: expected valid={sample.Item3}, observed {valid}.");
        }
    }

    private static void Set(object instance, string name, object value)
    {
        var property = instance.GetType().GetProperty(name);
        if (property?.CanWrite == true) property.SetValue(instance, value);
    }
}
